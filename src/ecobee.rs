use std::sync::Arc;

use actix::{Actor, Arbiter, Context, Handler};
use failure::{err_msg, Error};
use futures::{Future, IntoFuture, Stream};
use http::request::Builder;
use http::Request;
use hyper::client::HttpConnector;
use hyper::{Body, Client, Uri};
use hyper_tls::HttpsConnector;
use serde::de::DeserializeOwned;
use serde_json;
use serde_urlencoded;

use config::Config;
use query::EcobeeQuery;
use response::EcobeeResponse;
use Result;

trait FutureExt<I, E> {
    fn boxify(self) -> Box<dyn Future<Item = I, Error = E>>;
}

impl<I, E, F> FutureExt<I, E> for F
where
    F: Future<Item = I, Error = E> + 'static,
{
    fn boxify(self) -> Box<dyn Future<Item = I, Error = E>> {
        Box::new(self)
    }
}

#[derive(Deserialize, Debug)]
struct AuthToken {
    access_token: String,
    refresh_token: String,
}

#[derive(Deserialize, Debug)]
struct ErrorMessage {
    error: String,
    error_description: String,
}

#[derive(Debug, Fail)]
enum ErrorKind {
    #[fail(display = "remote error: {:?}", _0)]
    RemoteError(ErrorMessage),
}

pub struct EcobeeActor {
    client_id: String,
    client: Client<HttpsConnector<HttpConnector>, Body>,
    username: String,
    password: String,
    auth_token: Arc<Option<AuthToken>>,
}

impl EcobeeActor {
    const API_BASE: &'static str = "https://api.ecobee.com";

    fn build_client() -> Result<Client<HttpsConnector<HttpConnector>>> {
        let https = HttpsConnector::new(4)?;

        Ok(Client::builder().build::<_, Body>(https))
    }

    pub fn from_config(config: &Config) -> Result<Self> {
        Ok(Self {
            client_id: config.client_id.clone(),
            client: Self::build_client()?,
            username: config.username.clone(),
            password: config.password.clone(),
            auth_token: Arc::new(None),
        })
    }

    fn build_url(path: &str) -> Result<Uri> {
        format!("{}{}", Self::API_BASE, path)
            .parse()
            .map_err(Error::from)
    }

    fn send_request<R: DeserializeOwned + 'static>(
        &self,
        request: Request<Body>,
    ) -> Box<Future<Item = R, Error = Error>> {
        self.client
            .request(request)
            .and_then(|resp| resp.into_body().concat2())
            .map(|chunk| chunk.to_vec())
            .map_err(|e| -> Error { e.into() })
            .and_then(|data| {
                serde_json::from_slice(&data[..]).map_err(move |e| {
                    let error_message = serde_json::from_slice::<ErrorMessage>(&data[..]);

                    match error_message {
                        Ok(message) => ErrorKind::RemoteError(message).into(),
                        Err(_) => e.into(),
                    }
                })
            })
            .boxify()
    }

    fn auth(
        &mut self,
        username: String,
        password: String,
    ) -> impl Future<Item = AuthToken, Error = Error> {
        let payload = &[
            ("client_id", self.client_id.clone()),
            ("username", username),
            ("password", password),
            ("scope", "smartWrite".into()),
            ("response_type", "ecobeeAuthz".into()),
        ];
        let body = serde_json::to_string(payload).expect("serialized json");
        let query = serde_urlencoded::to_string(payload).expect("serialized query");

        if let Ok(url) = Self::build_url(&format!("/authorize?{}", query)) {
            let req = self.default_request(false).and_then(|mut req| {
                req.method("POST")
                    .uri(url)
                    .body(body.to_string().into())
                    .map_err(|e| e.into())
            });

            if let Ok(req) = req {
                self.send_request(req)
            } else {
                Err(err_msg("failed to build request"))
                    .into_future()
                    .boxify()
            }
        } else {
            Err(err_msg("failed to build url")).into_future().boxify()
        }
    }

    fn refresh_token(&mut self, refresh: String) -> impl Future<Item = AuthToken, Error = Error> {
        let payload = &[
            ("client_id", self.client_id.clone()),
            ("refresh_token", refresh),
            ("grant_type", "refresh_token".into()),
        ];
        let query = serde_urlencoded::to_string(payload).expect("serialize query");

        if let Ok(url) = Self::build_url(&format!("/token?{}", query)) {
            let req = self.default_request(true).and_then(|mut req| {
                req.method("POST")
                    .uri(url)
                    .body(Body::empty())
                    .map_err(|e| e.into())
            });

            if let Ok(req) = req {
                self.send_request(req)
            } else {
                Err(err_msg("failed to build request"))
                    .into_future()
                    .boxify()
            }
        } else {
            Err(err_msg("failed to build url")).into_future().boxify()
        }
    }

    fn default_request(&self, auth: bool) -> Result<Builder> {
        let mut builder = Request::builder();

        builder
            .header(
                "User-Agent",
                "Home Comfort/1.3.0 (iPhone; iOS 11.4; Scale/2.00)",
            )
            .header("X-ECOBEE-APP", "ecobee-ios");

        if auth {
            let token = Arc::try_unwrap(self.auth_token.clone())
                .map_err(|_| err_msg("unable to get auth token"))?
                .ok_or_else(|| err_msg("auth token is not set yet"))?;
            let value = format!("Bearer {}", token.access_token);

            builder.header("Authorization", &value[..]);
        }

        Ok(builder)
    }
}

impl Actor for EcobeeActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        let username = self.username.clone();
        let password = self.password.clone();
        let auth = self
            .auth(username, password)
            .map({
                let mut auth = self.auth_token.clone();
                move |token| *Arc::get_mut(&mut auth).unwrap() = Some(token)
            })
            .map_err(|err| {
                println!("{}", err);
            });

        Arbiter::spawn(auth);
    }
}

impl Handler<EcobeeQuery> for EcobeeActor {
    type Result = Result<EcobeeResponse>;

    fn handle(&mut self, _query: EcobeeQuery, _ctx: &mut Self::Context) -> Self::Result {
        Ok(EcobeeResponse::Status(1))
    }
}
