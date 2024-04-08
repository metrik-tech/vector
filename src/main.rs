use std::net::SocketAddr;

use agent::{Agent, DOCKER_UNIX_SOCK, LOCKFILE};
use serde::Deserialize;
use tide::{Request, Response, StatusCode};
use async_std::fs;

mod agent;

const DEPLOY_SECRET: &'static str = include_str!(concat!(env!("OUT_DIR"), "/secret.uuid"));
const PORT: u16 = 33293;

#[derive(Deserialize)]
struct DeployEndpointBody {
    secret: String,
}

#[async_std::main]
async fn main() {
    println!("DEPLOY_SECRET={}", DEPLOY_SECRET);

    let mut srv = tide::new();

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

        let mut agent = Agent::new(DOCKER_UNIX_SOCK, "hello-world").await.unwrap();
        agent.lock().await.unwrap();
        agent.deploy().await.unwrap();

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

    println!("Listening on port {}", PORT);

    srv.listen(SocketAddr::from(([127, 0, 0, 1], PORT)))
        .await
        .unwrap();
}
