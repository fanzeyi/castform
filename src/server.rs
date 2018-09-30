use actix::Addr;
use actix_web::http::StatusCode;
use actix_web::server::{HttpHandler, HttpHandlerTask};
use actix_web::{http, middleware, App, Error, HttpRequest, HttpResponse, Json, State};
use failure::err_msg;
use futures::{Future, IntoFuture};

use ecobee::EcobeeActor;
use query::EcobeeQuery;
use response::{EcobeeResponse, EcobeeStatus};

#[derive(Clone)]
struct HttpServerState {
    ecobee: Addr<EcobeeActor>,
}

fn status(state: State<HttpServerState>) -> impl Future<Item = Json<EcobeeStatus>, Error = Error> {
    state
        .ecobee
        .send(EcobeeQuery::Status)
        .map_err(|_| err_msg("mailbox error"))
        .flatten()
        .map(|resp: EcobeeResponse| match resp {
            EcobeeResponse::Status(status) => Json(status),
        })
        .from_err()
}

fn set_heating_cooling_state(
    state: State<HttpServerState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body("hello"))
    .into_future()
}

fn set_target_temperature(
    _: HttpRequest<HttpServerState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body("hello"))
    .into_future()
}

pub fn build_server_factory(
    ecobee: Addr<EcobeeActor>,
) -> impl IntoIterator<Item = Box<HttpHandler<Task = Box<HttpHandlerTask + 'static>> + 'static>> + 'static
{
    let state = HttpServerState { ecobee };
    vec![
        App::with_state(state)
            .middleware(middleware::Logger::default())
            .resource("/status", |r| {
                r.method(http::Method::GET).with_async(status)
            })
            .resource("/targetHeatingCoolingState", |r| {
                r.method(http::Method::POST)
                    .with_async(set_heating_cooling_state)
            })
            .resource("/targetTemperature", |r| {
                r.method(http::Method::POST)
                    .with_async(set_target_temperature)
            })
            .boxed(),
    ]
}
