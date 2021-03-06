extern crate actix;
#[macro_use]
extern crate actix_derive;
extern crate actix_web;
extern crate clap;
#[macro_use]
extern crate failure;
extern crate futures;
extern crate http;
extern crate hyper;
extern crate hyper_tls;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate serde_json;
extern crate serde_urlencoded;
extern crate tokio;
extern crate toml;

mod config;
mod ecobee;
mod query;
mod response;
mod server;

use std::fs::File;
use std::io::Read;

use actix::Actor;
use clap::{App, Arg};
use failure::{err_msg, Error};

use ecobee::EcobeeActor;

const VERSION: &'static str = "0.0.1";

pub type Result<R> = std::result::Result<R, Error>;

fn build_clap<'a, 'b>() -> App<'a, 'b> {
    clap::App::new("castform")
        .version(VERSION)
        .about("ecobee bridge to homekit")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("CONFIG_FILE")
                .default_value("config.toml")
                .help("path to config file"),
        )
        .arg(
            Arg::with_name("host")
                .short("H")
                .long("host")
                .value_name("HOST")
                .default_value("127.0.0.1")
                .help("HTTP host to listen to"),
        )
        .arg(
            Arg::with_name("port")
                .short("p")
                .long("port")
                .value_name("PORT")
                .default_value("8351")
                .help("HTTP port to listen to"),
        )
}

fn main() -> Result<()> {
    let system = actix::System::new("castform");

    let matches = build_clap().get_matches();

    let config = matches
        .value_of("config")
        .ok_or_else(|| err_msg("must provide config"))?;
    let mut config = File::open(config)?;
    let mut contents = String::new();
    config.read_to_string(&mut contents)?;

    let config = toml::from_str(&contents)?;

    let ecobee =
        EcobeeActor::from_config(&config).map(|actor| EcobeeActor::create(move |_| actor))?;
    let server = actix_web::server::new(move || server::build_server_factory(ecobee.clone()));

    let host = matches.value_of("host").unwrap();
    let port = matches.value_of("port").unwrap();
    let addr = format!("{}:{}", host, port);

    let server = server.bind(addr.clone())?;
    server.start();

    println!("Starting HTTP server: http://{}", addr);

    let _ = system.run();

    Ok(())
}
