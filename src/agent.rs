use std::path::Path;

use anyhow::Result;
use async_std::{fs, stream::StreamExt as _, task};
use docker_api::{
    opts::{ContainerCreateOptsBuilder, ContainerRemoveOptsBuilder, PublishPort, PullOptsBuilder},
    Container, Docker, Id,
};
use once_cell::sync::Lazy;
use rand::{distributions::Alphanumeric, Rng as _};
use serde::{Deserialize, Serialize};

pub const LOCKFILE: Lazy<&Path> = Lazy::new(|| Path::new("deploy.lock"));
pub const DOCKER_UNIX_SOCK: &'static str = "/var/run/docker.sock";

#[derive(Serialize, Deserialize, PartialEq)]
pub enum ContainerStatus {
    Running,
    Deploying,
}

#[derive(Serialize, Deserialize)]
pub struct AgentLockfile {
    container_id: Id,
    status: ContainerStatus,
}

#[derive(Debug)]
pub struct Agent {
    sock: Docker,
    container: Container,
    old_container: Option<Container>,
}

impl Agent {
    pub async fn new<T: ToString>(docker_sock: T, image: T) -> Result<Self> {
        let sock = Docker::unix(docker_sock.to_string());
        sock.ping().await?;
        let local_images = sock.images();
        let mut pull_stream = local_images.pull(
            &PullOptsBuilder::default()
                .image(image.to_string())
                .tag("latest")
                .build(),
        );

        while let Some(pull_res) = pull_stream.next().await {
            let _chunk = pull_res.map_err(anyhow::Error::from)?;
        }

        Ok(Self {
            sock: sock.clone(),
            container: sock
                .containers()
                .create(
                    &ContainerCreateOptsBuilder::new(format!(
                        "{}-{}",
                        image.to_string(),
                        rand::thread_rng()
                            .sample_iter(&Alphanumeric)
                            .take(7)
                            .map(char::from)
                            .collect::<String>()
                    ))
                    .image(image.to_string())
                    .expose(PublishPort::tcp(8080), 8080)
                    .build(),
                )
                .await?,

            old_container: None,
        })
    }

    pub async fn lock(&mut self) -> Result<&Self> {
        let lockfile = *LOCKFILE;

        if lockfile.exists() {
            let deserialized_lockfile =
                serde_json::from_str::<AgentLockfile>(&fs::read_to_string(lockfile).await?)?;

            if deserialized_lockfile.status == ContainerStatus::Deploying {
                let old_container_id = deserialized_lockfile.container_id.clone();
                eprintln!(
                    "previously abandoned deployment {} found, removing and redeploying",
                    &old_container_id.to_string()
                );
            }

            // FIXME: remove this clone
            self.old_container = Some(Container::new(
                self.sock.clone(),
                deserialized_lockfile.container_id,
            ))
        }

        generate_lockfile(lockfile, self.container.id(), ContainerStatus::Deploying).await?;

        Ok(&*self)
    }

    pub async fn deploy(mut self) -> Result<()> {
        match self.old_container.take().ok_or(anyhow::Error::msg(
            "agent needs to be locked before deploying",
        )) {
            Ok(container) => {
                fs::remove_file(*LOCKFILE).await?;
                remove_container(container).await
            },
            Err(err) => {
                if LOCKFILE.exists() {
                    return Err(err);
                }

                Ok(())
            }
        }?;

        task::spawn(async move {
            self.container.start().await?;
            generate_lockfile(*LOCKFILE, self.container.id(), ContainerStatus::Running).await?;

            Ok::<(), anyhow::Error>(())
        });

        Ok(())
    }
}

async fn remove_container(container: Container) -> Result<()> {
    container
        .remove(
            &ContainerRemoveOptsBuilder::default()
                .volumes(false)
                .force(true)
                .link(false)
                .build(),
        )
        .await?;

    Ok(())
}

async fn generate_lockfile(lockfile: &Path, id: &Id, status: ContainerStatus) -> Result<()> {
    let lockfile_contents = serde_json::to_string(&AgentLockfile {
        container_id: id.clone(),
        status: status,
    })?;

    fs::write(lockfile, lockfile_contents).await?;

    Ok(())
}
