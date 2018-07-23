use actix::{Actor, Context, Handler};

use config::Config;
use query::EcobeeQuery;
use response::EcobeeResponse;
use Result;

pub struct EcobeeActor {
    pub client_id: String,
}

impl EcobeeActor {
    pub fn from_config(config: &Config) -> Self {
        Self {
            client_id: config.client_id.clone(),
        }
    }
}

impl Actor for EcobeeActor {
    type Context = Context<Self>;
}

impl Handler<EcobeeQuery> for EcobeeActor {
    type Result = Result<EcobeeResponse>;

    fn handle(&mut self, query: EcobeeQuery, _ctx: &mut Self::Context) -> Self::Result {
        Ok(EcobeeResponse::Status(1))
    }
}
