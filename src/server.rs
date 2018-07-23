use actix_web::http::StatusCode;
use actix_web::server::{HttpHandler, HttpHandlerTask};
use actix_web::{http, middleware, App, Error, HttpRequest, HttpResponse};
use futures::{Future, IntoFuture};

fn status(_: HttpRequest) -> impl Future<Item = HttpResponse, Error = Error> {
    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body("hello"))
        .into_future()
}

pub fn build_server_factory(
) -> impl IntoIterator<Item = Box<HttpHandler<Task = Box<HttpHandlerTask + 'static>> + 'static>> + 'static
{
    vec![
        App::new()
            .middleware(middleware::Logger::default())
            .resource("/", |r| r.method(http::Method::GET).with_async(status))
            .boxed(),
    ]
}
