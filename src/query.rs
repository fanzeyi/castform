use actix::Message;

use response::EcobeeResponse;
use Result;

pub enum EcobeeQuery {
    Status,
}

impl Message for EcobeeQuery {
    type Result = Result<EcobeeResponse>;
}
