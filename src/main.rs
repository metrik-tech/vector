#![allow(
    clippy::borrow_interior_mutable_const,
    clippy::declare_interior_mutable_const
)]

use std::{env, net::SocketAddr, time::SystemTime};

use agent::{Agent, DOCKER_UNIX_SOCK, LOCKFILE};
use anyhow::Result;
use async_std::fs;
use log::info;
use owo_colors::OwoColorize;
use serde::Deserialize;
use tide::{Request, Response, StatusCode};

mod agent;

const DEPLOY_SECRET: &str = include_str!(concat!(env!("OUT_DIR"), "/secret.uuid"));
const PORT: u16 = 33293;

#[derive(Deserialize)]
struct DeployEndpointBody {
    secret: String,
}

#[async_std::main]
async fn main() -> Result<()> {
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} {} {} {}",
                humantime::format_rfc3339_seconds(SystemTime::now()).dimmed(),
                match record.level() {
                    log::Level::Error => "ERROR".red().to_string(),
                    log::Level::Warn => "WARN".yellow().to_string(),
                    log::Level::Info => "INFO".green().to_string(),
                    log::Level::Debug => "DEBUG".cyan().to_string(),
                    log::Level::Trace => "TRACE".blue().to_string(),
                },
                record.target().bold(),
                message
            ))
        })
        .level({
            if cfg!(debug_assertions) {
                log::LevelFilter::Trace
            } else {
                log::LevelFilter::Debug
            }
        })
        .chain(std::io::stdout())
        .chain(fern::log_file(format!(
            "/tmp/{}_{}.log",
            env::var("CARGO_PKG_NAME")?,
            humantime::format_rfc3339(SystemTime::now())
        ))?)
        .apply()?;

    info!("DEPLOY_SECRET={}", DEPLOY_SECRET);

    let mut srv = tide::new();
    srv.with(tide::log::LogMiddleware::new());

    srv.at("/deploy").post(|mut req: Request<()>| async move {
        let body =
            req.body_json::<DeployEndpointBody>()
                .await
                .map_err(|mut err: tide::Error| {
                    err.set_status(StatusCode::BadRequest);
                    err
                })?;

        if body.secret != DEPLOY_SECRET {
            return Err(tide::Error::new(
                StatusCode::Unauthorized,
                anyhow::Error::msg("invalid deploy secret"),
            ));
        }

        fn handle_agent_error(err: anyhow::Error) -> tide::Error {
            tide::Error::new(StatusCode::InternalServerError, err)
        }

        let mut agent = Agent::new(DOCKER_UNIX_SOCK, "hello-world")
            .await
            .map_err(handle_agent_error)?;
        agent.lock().await.map_err(handle_agent_error)?;
        agent.deploy().await.map_err(handle_agent_error)?;

        let resp = Response::new(StatusCode::Accepted);
        Ok(resp)
    });

    srv.at("/status").get(|_| async {
        Ok(serde_json::json!(fs::read_to_string(*LOCKFILE)
            .await
            .map_err(|err| tide::Error::new(
                StatusCode::InternalServerError,
                err
            ))?))
    });

    srv.listen(SocketAddr::from(([127, 0, 0, 1], PORT))).await?;

    Ok(())
}
