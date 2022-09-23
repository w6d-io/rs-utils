use std::{
    sync::Arc,
    marker::{Send, Sized, Sync},
    path::Path,
};

use anyhow::{anyhow, bail, Result};
use log::{debug, warn, info};
use notify::{
    event::{AccessKind, AccessMode, Event, EventKind},
    RecommendedWatcher, RecursiveMode, Watcher,
};
use tokio::{
    runtime::Handle,
    sync::{mpsc::{channel, Receiver}, RwLock, watch},
};

#[cfg(feature="kratos")]
pub use crate::kratos::Kratos;
#[cfg(feature="minio")]
pub use crate::minio::Minio;

pub trait Config: Send + Sync {
    fn new(env_var: &str) -> Self
    where
        Self: Sized;
    fn update<P: AsRef<Path>>(path: P) -> Result<Self>
    where
        Self: Sized;
}

///react to a file change
async fn event_reactor<P, C>(event: &Event, path: P, config: &Arc<RwLock<C>>, notif: &Option<watch::Sender<()>>) -> Result<()>
where
    P: AsRef<Path>,
    C: Config,
{
    if let EventKind::Access(AccessKind::Close(AccessMode::Write)) = event.kind {
        debug!("file changed: {:?}", event);
        let mut conf = config.write().await;
        *conf = Config::update(path)?;
        println!("sending change notiffication.");
        if let Some(n) = notif {
           println!("receiver:{}", n.receiver_count());
            n.send(())?;
        } 

    }
    Ok(())
}

#[allow(clippy::never_loop)]
///poll for file change event
async fn event_poll<P, C>(
    mut rx: Receiver<notify::Result<notify::Event>>,
    path: &P,
    config: &Arc<RwLock<C>>,
    notif: &Option<watch::Sender<()>>
) -> Result<()>
where
    P: AsRef<Path> + ?Sized + std::fmt::Debug,
    C: Config,
{
    info!("watching {path:?}");
    while let Some(event) = rx.recv().await {
        event_reactor(&event?, &path, config, notif).await?;
        #[cfg(test)]
        return Ok(());
    }
    Err(anyhow!("watch error: channel as been closed!"))
}

#[allow(unused_variables)]
///watch the config file for wrtie event and update the internal config data
async fn config_watcher<P, C>(
    path: P,
    config: &Arc<RwLock<C>>,
    notif: &Option<watch::Sender<()>>
    ) -> Result<()>
where
    P: AsRef<Path> + std::fmt::Debug,
    C: Config,
{
    let (tx, rx) = channel(1);
    let handle = Handle::current();
    // Automatically select the best implementation for your platform.
    // You can also access each implementation directly e.g. INotifyWatcher.
    let mut watcher = RecommendedWatcher::new(move |res| {
        handle.block_on(async {
            tx.send(res)
                .await
                .expect("something went wrong with the watcher channel");
        })
    }, notify::Config::default())?;
    watcher.watch(path.as_ref(), RecursiveMode::Recursive)?;
    #[cfg(not(test))]
    if let Err(err) = event_poll(rx, &path, config, notif).await {
        warn!(
            "an error occured in the watcher: {:?}\n trying to reload",
            err
        );
    };
    Ok(())
}

///ititialise the config watchers
///use the otional argument notif to reseiv notification of update
pub async fn init_watcher<P, C>(
    path: P,
    config: Arc<RwLock<C>>,
    notif: Option<watch::Sender<()>>
    ) -> Result<()>
where
    P: AsRef<Path> + std::fmt::Debug,
    C: Config,
{
    info!("initialising_watcher");
    if !path.as_ref().exists() {
        bail!("no file found at {:?}", path.as_ref());
    }

    loop {
        config_watcher(&path, &config, &notif).await?;
    }
}

#[cfg(test)]
mod test_config {
    use super::*;
    use figment::{
        providers::{Format, Yaml},
        Figment,
    };
    use serde::Deserialize;
    use std::collections::HashMap;

    #[derive(Deserialize)]
    pub struct TestConfig {
        pub salt: String,
        pub salt_length: usize,
        pub http: HashMap<String, String>,
        pub grpc: HashMap<String, String>,
    }

    const PATH: &str = "test/config.yaml";

    impl Config for TestConfig {
        ///initialise the config struct
        fn new(var: &str) -> Self {
            let path = match std::env::var(var) {
                Ok(path) => path,
                Err(e) => {
                    warn!("error while reading environment variable: {e}, switching to fallback.");
                    PATH.to_owned()
                }
            };
            match Self::update(&path) {
                Ok(conf) => conf,
                Err(e) => panic!("failed to update config {:?}: {:?}", path, e),
            }
        }

        ///update the config in the static variable
        fn update<P: AsRef<Path>>(path: P) -> Result<Self> {
            if !path.as_ref().exists() {
                bail!("config was not found");
            }
            let config: TestConfig = Figment::new().merge(Yaml::file(path)).extract()?;
            Ok(config)
        }
    }


    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_event_reactor() {
        let path = PATH;
        let event = notify::event::Event {
            kind: EventKind::Access(AccessKind::Close(AccessMode::Write)),
            paths: vec![Path::new(path).to_path_buf()],
            attrs: notify::event::EventAttributes::new(),
        };
        let config = Arc::new(RwLock::new(TestConfig::new("CONFIG")));
        event_reactor(&event, &path, &config.clone(), &None).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_event_poll() {
        let config = Arc::new(RwLock::new(TestConfig::new("CONFIG")));
        let (tx, rx) = channel(1);
        let path = PATH;
        let event = notify::event::Event {
            kind: EventKind::Access(AccessKind::Close(AccessMode::Write)),
            paths: vec![Path::new(path).to_path_buf()],
            attrs: notify::event::EventAttributes::new(),
        };
        tx.send(Ok(event)).await.unwrap();
        event_poll(rx, &path, &config.clone(), &None).await.unwrap();
    }

    #[tokio::test]
    async fn test_event_poll_closed_chanel() {
        let config = Arc::new(RwLock::new(TestConfig::new("CONFIG")));
        let (tx, rx) = channel(1);
        let path = PATH;
        drop(tx);
        let res = event_poll(rx, path, &config.clone(), &None).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_config_watcher() {
        let config = Arc::new(RwLock::new(TestConfig::new("CONFIG")));
        let res = config_watcher(PATH, &config.clone(), &None).await;
        assert!(res.is_ok());
    }
}
