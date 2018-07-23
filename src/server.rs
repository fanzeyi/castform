use actix::Addr;
use actix_web::http::StatusCode;
use actix_web::server::{HttpHandler, HttpHandlerTask};
use actix_web::{http, middleware, App, Error, HttpRequest, HttpResponse};
use futures::{Future, IntoFuture};

use ecobee::EcobeeActor;

#[derive(Clone)]
struct HttpServerState {
    ecobee: Addr<EcobeeActor>,
}

fn status(_: HttpRequest<HttpServerState>) -> impl Future<Item = HttpResponse, Error = Error> {
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
            .resource("/", |r| r.method(http::Method::GET).with_async(status))
            .boxed(),
    ]
}
