use std::{
    marker::{Send, Sized, Sync},
    path::Path,
};

use anyhow::{anyhow, bail, Result};
use log::{debug, warn, info};
use notify::{
    event::{AccessKind, AccessMode, Event, EventKind},
    RecommendedWatcher, RecursiveMode, Watcher,
};
use once_cell::sync::Lazy;
use tokio::{
    runtime::Handle,
    sync::{mpsc::{channel, Receiver}, RwLock, watch},
};

#[cfg(feature="kratos")]
pub mod kratos{
    use ory_kratos_client::apis::configuration::Configuration;
    use serde::Deserialize;

    ///structure containing kratos config. thi to be used with figment
    #[derive(Deserialize)]
    pub struct Kratos {
        pub addr: String,

        #[serde(skip)]
        pub client: Option<Configuration>,
    }

    impl Kratos {
        ///this fuction update the kratos client
        pub fn update_kratos(mut self) -> Self {
            let kratos = &self;
            let mut client = Configuration::new();
            client.base_path = kratos.addr.clone();
            self.client = Some(client);
            self
        }
    }
}
#[cfg(feature="kratos")]
pub use kratos::Kratos;

pub trait Config: Send + Sync {
    fn new(env_var: &str) -> Self
    where
        Self: Sized;
    fn update<P: AsRef<Path>>(path: P) -> Result<Self>
    where
        Self: Sized;
}

///react to a file change
async fn event_reactor<P, C>(event: &Event, path: P, config: &Lazy<RwLock<C>>, notif: &Option<watch::Sender<()>>) -> Result<()>
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
    config: &Lazy<RwLock<C>>,
    notif: &Option<watch::Sender<()>>
) -> Result<()>
where
    P: AsRef<Path> + ?Sized,
    C: Config,
{
    while let Some(event) = rx.recv().await {
        event_reactor(&event?, &path, config, notif).await?;
        #[cfg(est)]
        return Ok(());
    }
    Err(anyhow!("watch error: channel as been closed!"))
}

///watch the config file for wrtie event and update the internal config data
async fn config_watcher<P, C>(
    path: P,
    config: &Lazy<RwLock<C>>,
    notif: &Option<watch::Sender<()>>
    ) -> Result<()>
where
    P: AsRef<Path>,
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
    })?;
    watcher.watch(path.as_ref(), RecursiveMode::Recursive)?;
    // #[cfg(not(test))]
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
    config: &Lazy<RwLock<C>>,
    notif: Option<watch::Sender<()>>
    ) -> Result<()>
where
    P: AsRef<Path>,
    C: Config,
{
    if !path.as_ref().exists() {
        bail!("no file found at {:?}", path.as_ref());
    }

    loop {
        config_watcher(&path, config, &notif).await?;
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

    pub static CONFIG: Lazy<RwLock<TestConfig>> = Lazy::new(|| RwLock::new(Config::new("CONFIG")));

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_event_reactor() {
        let path = PATH;
        let event = notify::event::Event {
            kind: EventKind::Access(AccessKind::Close(AccessMode::Write)),
            paths: vec![Path::new(path).to_path_buf()],
            attrs: notify::event::EventAttributes::new(),
        };
        event_reactor(&event, &path, &CONFIG, &None).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_event_poll() {
        let (tx, rx) = channel(1);
        let path = PATH;
        let event = notify::event::Event {
            kind: EventKind::Access(AccessKind::Close(AccessMode::Write)),
            paths: vec![Path::new(path).to_path_buf()],
            attrs: notify::event::EventAttributes::new(),
        };
        tx.send(Ok(event)).await.unwrap();
        event_poll(rx, &path, &CONFIG, &None).await.unwrap();
    }

    #[tokio::test]
    async fn test_event_poll_closed_chanel() {
        let (tx, rx) = channel(1);
        let path = PATH;
        drop(tx);
        let res = event_poll(rx, path, &CONFIG, &None).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_config_watcher() {
        let res = config_watcher(PATH, &CONFIG, &None).await;
        assert!(res.is_ok());
    }
}
