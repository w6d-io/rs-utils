use std::{marker::Sized, path::Path, sync::Arc, thread::sleep, time::Duration};

use anyhow::{anyhow, bail, Result};
use async_trait::async_trait;
#[cfg(not(test))]
use log::warn;
use log::{debug, info, error};
use notify::{
    event::{AccessKind, AccessMode, Event, EventKind},
    RecommendedWatcher, RecursiveMode, Watcher,
};
use serde::Deserialize;
use tokio::{
    runtime::Handle,
    sync::{
        mpsc::{channel, Receiver},
        watch, RwLock,
    },
};

#[cfg(feature = "kratos")]
pub use crate::kratos::Kratos;
#[cfg(feature = "minio")]
pub use crate::minio::Minio;
#[cfg(feature = "redis")]
pub use crate::redis::Redis;

#[async_trait]
pub trait Config: Default {
    async fn new(path: &str) -> Self
    where
        Self: Sized + for<'a> Deserialize<'a>,
    {
        let mut config = Self::default();
        config.set_path(path);
        let mut duration = 30;
        for retry in 0..4 {
            if retry > 0 {
                sleep(Duration::from_secs(duration));
                duration = duration + duration * retry;
                info!("trying to reload config. retry:{}", retry);
            }
            if let Err(e) = config.update().await {
                if retry > 3{
                    panic!("failed to load config after 3e try {:?}: {:?}", path, e)
                } else {
                    error!("failed to load config {:?}: {:?}", path, e);
                    info!("waiting {}s before reloading.", duration);
                }
            }else {
                info!("{:?}", config);
                break;
            }

        }
        config
    }

    fn set_path<T: AsRef<Path>>(&mut self, path: T) -> &mut Self;
    async fn update(&mut self) -> Result<()>
    where
        Self: Sized;
}

///react to a file change
async fn event_reactor<C>(
    event: &Event,
    config: &Arc<RwLock<C>>,
    notif: &Option<watch::Sender<()>>,
) -> Result<()>
where
    C: Config,
{
    if let EventKind::Access(AccessKind::Close(AccessMode::Write)) = event.kind {
        debug!("file changed: {:?}", event);
        let mut conf = config.write().await;
        conf.update().await?;
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
async fn event_poll<C>(
    mut rx: Receiver<notify::Result<notify::Event>>,
    config: &Arc<RwLock<C>>,
    notif: &Option<watch::Sender<()>>,
) -> Result<()>
where
    C: Config,
{
    while let Some(event) = rx.recv().await {
        event_reactor(&event?, config, notif).await?;
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
    notif: &Option<watch::Sender<()>>,
) -> Result<()>
where
    P: AsRef<Path> + std::fmt::Debug,
    C: Config,
{
    let (tx, rx) = channel(1);
    let handle = Handle::current();
    // Automatically select the best implementation for your platform.
    // You can also access each implementation directly e.g. INotifyWatcher.
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            handle.block_on(async {
                tx.send(res)
                    .await
                    .expect("something went wrong with the watcher channel");
            })
        },
        notify::Config::default(),
    )?;
    watcher.watch(path.as_ref(), RecursiveMode::Recursive)?;
    #[cfg(not(test))]
    if let Err(err) = event_poll(rx, config, notif).await {
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
    notif: Option<watch::Sender<()>>,
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
    use std::path::PathBuf;

    use figment::{
        providers::{Format, Yaml},
        Figment,
    };
    use log::warn;

    use super::*;

    #[derive(Deserialize, Default, Clone, PartialEq, Eq, Debug)]
    pub struct TestConfig {
        pub salt: String,
        pub salt_length: usize,
        path: Option<PathBuf>,
    }

    const PATH: &str = "test/config.yaml";

    #[async_trait]
    impl Config for TestConfig {
        fn set_path<T: AsRef<Path>>(&mut self, path: T) -> &mut Self {
            self.path = Some(path.as_ref().to_owned());
            self
        }

        ///update the config in the static variable
        async fn update(&mut self) -> Result<()> {
            let path = match self.path {
                Some(ref path) => path as &Path,
                None => bail!("config file path not set"),
            };
            match path.try_exists() {
                Ok(exists) => {
                    if !exists {
                        bail!("config was not found");
                    }
                }
                Err(e) => bail!(e),
            }
            let mut figment: TestConfig = Figment::new().merge(Yaml::file(path)).extract()?;
            figment.path = Some(path.to_path_buf());
            *self = figment;
            Ok(())
        }

        /* fn new(env_var: &str) -> Self
        where
            Self: Sized + for<'a> Deserialize<'a>,
        {
            let path = match std::env::var(env_var) {
                Ok(path) => path,
                Err(e) => {
                    warn!("error while reading environment variable: {e}, switching to fallback.");
                    "tests/config.toml".to_owned()
                }
            };
            let mut config = Self::default();
            config.set_path(path.clone());
            if let Err(e) = config.update() {
                panic!("failed to update config {:?}: {:?}", path, e);
            };
            config
        } */
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_config_new() {
        let expected = TestConfig {
            salt: "test".to_owned(),
            salt_length: 200,
            path: Some(PathBuf::from("test/config.yaml")),
        };
        std::env::set_var("CONFIG", PATH);
        let config_path = std::env::var("CONFIG").unwrap_or_else(|_| {
            warn!("config variable not found switching to fallback");
            PATH.to_owned()
        });
        let config = TestConfig::new(&config_path).await;
        assert_eq!(config, expected)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_event_reactor() {
        std::env::set_var("CONFIG", PATH);
        let config_path = std::env::var("CONFIG").unwrap_or_else(|_| {
            warn!("config variable not found switching to fallback");
            PATH.to_owned()
        });
        let path = PATH;
        let event = notify::event::Event {
            kind: EventKind::Access(AccessKind::Close(AccessMode::Write)),
            paths: vec![Path::new(path).to_path_buf()],
            attrs: notify::event::EventAttributes::new(),
        };
        let config = Arc::new(RwLock::new(TestConfig::new(&config_path).await));
        event_reactor(&event, &config.clone(), &None).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_event_poll() {
        std::env::set_var("CONFIG", PATH);
        let config_path = std::env::var("CONFIG").unwrap_or_else(|_| {
            warn!("config variable not found switching to fallback");
            PATH.to_owned()
        });
        let config = Arc::new(RwLock::new(TestConfig::new(&config_path).await));
        let (tx, rx) = channel(1);
        let path = PATH;
        let event = notify::event::Event {
            kind: EventKind::Access(AccessKind::Close(AccessMode::Write)),
            paths: vec![Path::new(path).to_path_buf()],
            attrs: notify::event::EventAttributes::new(),
        };
        tx.send(Ok(event)).await.unwrap();
        event_poll(rx, &config.clone(), &None).await.unwrap();
    }

    #[tokio::test]
    async fn test_event_poll_closed_chanel() {
        std::env::set_var("CONFIG", PATH);
        let config_path = std::env::var("CONFIG").unwrap_or_else(|_| {
            warn!("config variable not found switching to fallback");
            PATH.to_owned()
        });
        let config = Arc::new(RwLock::new(TestConfig::new(&config_path).await));
        let (tx, rx) = channel(1);
        drop(tx);
        let res = event_poll(rx, &config.clone(), &None).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_config_watcher() {
        std::env::set_var("CONFIG", PATH);
        let config_path = std::env::var("CONFIG").unwrap_or_else(|_| {
            warn!("config variable not found switching to fallback");
            PATH.to_owned()
        });
        let config = Arc::new(RwLock::new(TestConfig::new(&config_path).await));
        let res = config_watcher(PATH, &config.clone(), &None).await;
        assert!(res.is_ok());
    }
}
