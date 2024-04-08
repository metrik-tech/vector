use constime::comptime;
use serde::Deserialize;
use tide::{Request, Response, StatusCode};
use agent::Agent;

use crate::agent::DOCKER_UNIX_SOCK;

mod agent;

const DEPLOY_SECRET: &'static str = include_str!(concat!(env!("OUT_DIR"), "/secret.uuid"));

#[derive(Deserialize)]
struct DeployEndpointBody {
    secret: String,
}

#[tokio::main]
async fn main() {
    println!("DEPLOY_SECRET={}", DEPLOY_SECRET);

    // let mut srv = tide::new();

    // srv.at("/deploy").post(|mut req: Request<()>| async move {
    //     let body =
    //         req.body_json::<DeployEndpointBody>()
    //             .await
    //             .map_err(|mut err: tide::Error| {
    //                 err.set_status(StatusCode::BadRequest);
    //                 err
    //             })?;

    //     if body.secret != DEPLOY_SECRET {
    //         return Err(tide::Error::new(
    //             StatusCode::Unauthorized,
    //             anyhow::Error::msg("invalid deploy secret"),
    //         ));
    //     }

    //     let mut resp = Response::new(StatusCode::Accepted);
    //     resp.set_body(serde_json::json!({
    //         "error": None::<()>,
    //     }));

    //     Ok(resp)
    // });

        let mut agent = Agent::new(DOCKER_UNIX_SOCK.to_string(), "hello-world".to_string()).await.unwrap();
        agent.lock().await.unwrap();
        agent.deploy().await.unwrap();
}
