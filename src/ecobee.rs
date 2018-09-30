use std::collections::HashMap;
use std::time::Duration;

use actix::{Actor, Arbiter, AsyncContext, Context, Handler};
use failure::{err_msg, Error};
use futures::{Future, IntoFuture, Stream};
use http::request::Builder;
use http::Request;
use hyper::client::HttpConnector;
use hyper::{Body, Client, Uri};
use hyper_tls::HttpsConnector;
use serde::de::DeserializeOwned;
use serde_json;
use serde_json::Value;
use serde_urlencoded;

use config::Config;
use query::EcobeeQuery;
use response::{EcobeeResponse, EcobeeStatus};
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

fn ftoc(f: f32) -> f32 {
    (f - 32.0) / 1.8
}

#[derive(Deserialize, Clone, Debug)]
struct AuthToken {
    access_token: String,
    refresh_token: String,
}

#[derive(Deserialize, Debug)]
struct ErrorMessage {
    error: String,
    error_description: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ThermostatRuntime {
    #[serde(rename = "actualTemperature")]
    temperature: usize,
    #[serde(rename = "actualHumidity")]
    humidity: usize,
    desired_heat: usize,
    desired_cool: usize,
    desired_humidity: usize,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ThermostatSettings {
    hvac_mode: String,
}

#[derive(Deserialize, Debug)]
struct Thermostat {
    runtime: ThermostatRuntime,
    settings: ThermostatSettings,
    #[serde(flatten)]
    other: HashMap<String, Value>,
}

#[derive(Deserialize, Debug)]
struct ThermostatResponse {
    #[serde(rename = "thermostatList")]
    thermostats: Vec<Thermostat>,
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
    auth_token: Option<AuthToken>,
    thermostats: Vec<Thermostat>,
}

impl EcobeeActor {
    const API_BASE: &'static str = "https://api.ecobee.com";

    fn build_client() -> Result<Client<HttpsConnector<HttpConnector>>> {
        let https = HttpsConnector::new(4)?;

        Ok(Client::builder().build::<_, Body>(https))
    }

    fn build_url(path: &str, payload: Vec<(&str, String)>) -> Result<Uri> {
        let url = if payload.is_empty() {
            format!("{}{}", Self::API_BASE, path)
        } else {
            let query: String = serde_urlencoded::to_string(&payload).map_err(Error::from)?;
            format!("{}{}?{}", Self::API_BASE, path, query)
        };

        url.parse().map_err(From::from)
    }

    pub fn from_config(config: &Config) -> Result<Self> {
        Ok(Self {
            client_id: config.client_id.clone(),
            client: Self::build_client()?,
            username: config.username.clone(),
            password: config.password.clone(),
            auth_token: None,
            thermostats: Vec::new(),
        })
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
        &self,
        username: String,
        password: String,
    ) -> impl Future<Item = AuthToken, Error = Error> {
        let payload = [
            ("client_id", self.client_id.clone()),
            ("username", username),
            ("password", password),
            ("scope", "smartWrite".into()),
            ("response_type", "ecobeeAuthz".into()),
        ];
        let body = serde_json::to_string(&payload).expect("serialized json");
        let req = Self::build_url("/authorize", payload.to_vec()).and_then(|url| {
            self.default_request(false).and_then(|mut req| {
                req.method("POST")
                    .uri(url)
                    .body(body.into())
                    .map_err(|e| e.into())
            })
        });

        match req {
            Ok(req) => self.send_request(req),
            Err(err) => Err(err_msg(format!("failed to build the request: {:?}", err)))
                .into_future()
                .boxify(),
        }
    }

    fn refresh_token(&self, refresh: String) -> impl Future<Item = AuthToken, Error = Error> {
        let payload = [
            ("client_id", self.client_id.clone()),
            ("refresh_token", refresh),
            ("grant_type", "refresh_token".into()),
        ];

        let req = Self::build_url("/token", payload.to_vec()).and_then(|url| {
            self.default_request(false).and_then(|mut req| {
                req.method("POST")
                    .uri(url)
                    .body(Body::empty())
                    .map_err(|e| e.into())
            })
        });

        match req {
            Ok(req) => self.send_request(req),
            Err(err) => Err(err_msg(format!("failed to build the request: {:?}", err)))
                .into_future()
                .boxify(),
        }
    }

    fn get_thermostat(&self) -> impl Future<Item = ThermostatResponse, Error = Error> {
        let payload = [
            ("json", r#"{"selection":{"includeOemCfg":"true","includeAlerts":"true","includeVersion":"true","includeLocation":"true","selectionType":"registered","includeEvents":"true","includeHouseDetails":"true","includeRuntime":"true","includeNotificationSettings":"true","includeProgram":"true","includeWeather":"true","includePrivacy":"true","includeSecuritySettings":"true","includeSettings":"true","includeExtendedRuntime":"true","includeSensors":"true","includeTechnician":"true"}}"#.into())
        ];

        let req = Self::build_url("/1/thermostat", payload.to_vec()).and_then(|url| {
            self.default_request(true).and_then(|mut req| {
                req.method("GET")
                    .uri(url)
                    .body(Body::empty())
                    .map_err(|e| e.into())
            })
        });

        match req {
            Ok(req) => self.send_request(req),
            Err(err) => Err(err_msg(format!("failed to build the request: {:?}", err)))
                .into_future()
                .boxify(),
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
            let token = self
                .auth_token
                .clone()
                .ok_or_else(|| err_msg("auth token is not set yet"))?;
            let value = format!("Bearer {}", token.access_token);

            builder.header("Authorization", &value[..]);
        }

        Ok(builder)
    }
}

impl Actor for EcobeeActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let username = self.username.clone();
        let password = self.password.clone();
        let addr = ctx.address();
        let auth = self
            .auth(username, password)
            .and_then(move |token| {
                addr.try_send(SetAuthToken(token))
                    .map_err(|_| err_msg("send error"))
            })
            .map_err(|err| {
                println!("{}", err);
            });

        Arbiter::spawn(auth);

        ctx.run_interval(Duration::from_secs(60 * 60 * 24), |actor, context| {
            if let Some(token) = actor.auth_token.clone() {
                let addr = context.address();
                println!("refreshing token...");
                let refresh = actor
                    .refresh_token(token.refresh_token)
                    .map(move |token| {
                        if let Err(_) = addr.try_send(SetAuthToken(token)) {
                            eprintln!("send failed.");
                        }
                    })
                    .map_err(|e| {
                        eprintln!("error occurred when refreshing token: {:?}", e);
                    });

                Arbiter::spawn(refresh);
            }
        });

        ctx.run_interval(Duration::from_secs(60), |actor, context| {
            let addr = context.address();
            let fut = actor
                .get_thermostat()
                .map(move |thermostat| {
                    if let Err(_) = addr.try_send(UpdateThermostat(thermostat)) {
                        eprintln!("send failed.");
                    }
                })
                .map_err(|e| {
                    eprintln!("error occurred when fetching thermostat: {:?}", e);
                });

            Arbiter::spawn(fut);
        });
    }
}

impl Handler<EcobeeQuery> for EcobeeActor {
    type Result = Result<EcobeeResponse>;

    fn handle(&mut self, _query: EcobeeQuery, _ctx: &mut Self::Context) -> Self::Result {
        if let Some(thermostat) = self.thermostats.first() {
            let mode: u8 = match &thermostat.settings.hvac_mode[..] {
                "auto" => 3,
                "cool" => 2,
                "heat" => 1,
                _ => 0,
            };
            let runtime = &thermostat.runtime;
            let target: f32 = {
                let heat = runtime.desired_heat as f32;
                let cool = runtime.desired_cool as f32;
                (heat + cool) / 20.0
            };
            let current: f32 = (runtime.temperature as f32) / 10.0;
            let humidity: f32 = runtime.humidity as f32;
            let target_humidity: f32 = runtime.desired_humidity as f32;

            Ok(EcobeeResponse::Status(EcobeeStatus::new(
                mode,
                ftoc(target),
                ftoc(current),
                humidity,
                target_humidity / 100.0,
            )))
        } else {
            Err(err_msg("no thermostat available"))
        }
    }
}

#[derive(Message)]
struct UpdateThermostat(ThermostatResponse);

impl Handler<UpdateThermostat> for EcobeeActor {
    type Result = ();

    fn handle(&mut self, update: UpdateThermostat, _: &mut Self::Context) -> Self::Result {
        self.thermostats = update.0.thermostats;
    }
}

#[derive(Message)]
struct SetAuthToken(AuthToken);

impl Handler<SetAuthToken> for EcobeeActor {
    type Result = ();

    fn handle(&mut self, request: SetAuthToken, _: &mut Self::Context) -> Self::Result {
        println!("setting token to: {:?}", request.0);
        self.auth_token = Some(request.0.clone());
    }
}
