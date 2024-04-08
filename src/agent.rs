use std::path::Path;

use anyhow::Result;
use docker_api::{
    opts::{ContainerCreateOptsBuilder, ContainerRemoveOptsBuilder, PublishPort},
    Container, Docker, Id,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::{fs, task};

const LOCKFILE: Lazy<&Path> = Lazy::new(|| Path::new("deploy.lock"));
pub const DOCKER_UNIX_SOCK: &'static str = "/var/run/docker.sock";

#[derive(Serialize, Deserialize)]
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
    pub async fn new(docker_sock: String, image: String) -> Result<Self> {
        let sock = Docker::unix(docker_sock);
        sock.ping().await?;

        Ok(Self {
            sock: sock.clone(),
            container: sock
                .containers()
                .create(
                    &ContainerCreateOptsBuilder::new("metrik-web")
                        .image(image)
                        .expose(PublishPort::tcp(8080), 8080)
                        .build(),
                )
                .await?,

            old_container: None,
        })
    }

    pub async fn lock(&mut self) -> Result<()> {
        let lockfile = *LOCKFILE;

        if lockfile.exists() {
            let deserialized_lockfile =
                serde_json::from_str::<AgentLockfile>(&fs::read_to_string(lockfile).await?)?;
            // FIXME: remove this clone
            self.old_container = Some(Container::new(
                self.sock.clone(),
                deserialized_lockfile.container_id,
            ))
        }

        generate_lockfile(lockfile, self.container.id(), ContainerStatus::Deploying).await?;

        Ok(())
    }

    pub async fn deploy(mut self) -> Result<()> {
        let old_container = self.old_container.take().ok_or(anyhow::Error::msg(
            "agent needs to be locked before deploying",
        ))?;

        old_container
            .remove(
                &ContainerRemoveOptsBuilder::default()
                    .volumes(false)
                    .force(true)
                    .link(false)
                    .build(),
            )
            .await?;

        task::spawn(async move {
            self.container.start().await?;
            generate_lockfile(*LOCKFILE, self.container.id(), ContainerStatus::Running).await?;

            Ok::<(), anyhow::Error>(())
        });

        Ok(())
    }
}

async fn generate_lockfile(lockfile: &Path, id: &Id, status: ContainerStatus) -> Result<()> {
    let lockfile_contents = serde_json::to_string(&AgentLockfile {
        container_id: id.clone(),
        status: status,
    })?;

    fs::write(lockfile, lockfile_contents).await?;

    Ok(())
}
