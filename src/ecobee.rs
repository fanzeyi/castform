use config::Config;

pub struct EcobeeActor {
    pub client_id: String,
}

impl EcobeeActor {
    pub fn from_config(config: &Config) -> EcobeeActor {
        EcobeeActor {
            client_id: config.client_id.clone(),
        }
    }
}
