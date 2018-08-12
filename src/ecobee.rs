use actix::{Actor, Arbiter, Context, Handler};
use failure::{err_msg, Error};
use futures::{Future, IntoFuture, Stream};
use http::request::Builder;
use http::Request;
use hyper::client::HttpConnector;
use hyper::{Body, Client, Response, Uri};
use hyper_tls::HttpsConnector;
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

pub struct EcobeeActor {
    pub client_id: String,
    client: Client<HttpsConnector<HttpConnector>, Body>,
    username: String,
    password: String,
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
        })
    }

    fn build_url(path: &str) -> Result<Uri> {
        format!("{}{}", Self::API_BASE, path)
            .parse()
            .map_err(Error::from)
    }

    fn auth(&mut self) -> impl Future<Item = Response<Body>, Error = Error> {
        let payload = &[
            ("client_id", self.client_id.clone()),
            ("username", self.username.clone()),
            ("password", self.password.clone()),
            ("scope", "smartWrite".into()),
            ("response_type", "ecobeeAuthz".into()),
        ];
        let body = serde_json::to_string(payload).expect("serialized json");
        let query = serde_urlencoded::to_string(payload).expect("serialized query");

        if let Ok(url) = Self::build_url(&format!("/authorize?{}", query)) {
            let mut req = self.default_request();
            if let Ok(req) = req.method("POST").uri(url).body(body.to_string().into()) {
                self.client.request(req).from_err().boxify()
            } else {
                Err(err_msg("failed to build request"))
                    .into_future()
                    .boxify()
            }
        } else {
            Err(err_msg("failed to build url")).into_future().boxify()
        }
    }

    fn default_request(&self) -> Builder {
        let mut builder = Request::builder();

        builder
            .header(
                "User-Agent",
                "Home Comfort/1.3.0 (iPhone; iOS 11.4; Scale/2.00)",
            )
            .header("X-ECOBEE-APP", "ecobee-ios");

        builder
    }
}

impl Actor for EcobeeActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        let auth = self
            .auth()
            .and_then(|resp| resp.into_body().concat2().from_err())
            .map(|resp| {
                println!("resp: {:?}", unsafe {
                    String::from_utf8_unchecked(resp.to_vec())
                })
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
