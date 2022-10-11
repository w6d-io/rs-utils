use std::{env, fmt::Debug};

use log::warn;
use redis::{Cmd, aio::ConnectionManager, aio::Connection};
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RedisError {
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("Not yet connected to the redis server.")]
    Connection,
    #[error("provided redis user without password")]
    NoPassword
}

type Result<T> = std::result::Result<T, RedisError>;

#[derive(Debug, Deserialize, Clone, Default)]
pub struct Redis {
    pub addr: String,
    pub password: Option<String>,
    pub user: Option<String>,
    #[serde(skip_deserializing)]
    pub client: Option<Client>,
    #[serde(skip_deserializing)]
    pub prefix: Option<String>,
}

impl Redis {
    pub fn update(&mut self) -> Result<&mut Self> {
        let client = Client::new(self)?;
        self.client = Some(client);
        Ok(self)
    }

     ///fetch the secret from the environment
    pub fn set_secrets(&mut self) -> &mut Self {
        let prefix = match self.prefix {
            Some(ref pref) => pref.to_owned(),
            None => {
                warn!("No prefix provided!");
                return self;
            }
        };
        self.password = env::var(prefix + "_REDIS_PASSWORD")
            .ok()
            .or_else(|| {
                if self.password.is_some() {
                    self.password.to_owned()
                } else {
                    None
                }
            });
        self
    }
}

#[derive(Clone)]
pub struct Client{
    client: redis::Client,
    pub connection: Option<ConnectionManager>
}

impl Client {

    ///create a new client form the redis config
    pub fn new(config: &Redis) -> Result<Self> {
        let mut url = String::from("redis://");
        match config.password {
            Some(ref password) => {
                if let Some(ref user) = config.user {
                    url = url + user as &str + ":";
                }
                url = url + password as &str + "@";
            }
            None => {
                if config.user.is_some() {
                    return Err(RedisError::NoPassword);
                }
            }
        }
        url += &config.addr as &str;

        let info = redis::Client::open(config.addr.clone())?;
        let client = Client{
            client: info,
            connection: None
        };
        Ok(client)
    }
    ///Return a simple connection , this connection is not managed,
    pub async fn get_simple_connection(&self) -> Result<Connection>{
        let conection = self.client.get_tokio_connection().await?;
        Ok(conection)
    }
    ///Connect the client to the redis server, it will try to reconnect
    ///automatically if an error is encountered.
    pub async fn connect(&mut self) -> Result<&mut Self>{
        let conection = self.client.get_tokio_connection_manager().await?;
        self.connection = Some(conection);
        Ok(self)
    }
    ///hset redis command
    pub async fn hset(&self, key: &str, field: &str, value: &str) -> Result<()> {
        let mut connection = match self.connection{
            Some(ref connection) => connection.clone(),
            None => return Err(RedisError::Connection)
        };
        Cmd::hset_nx(key, field, value)
            .query_async(&mut connection)
            .await?;
        Ok(())
    }

    ///hexists redis command
    pub async fn hexists(&self, key: &str, field: &str) -> Result<bool> {
        let mut connection = match self.connection{
            Some(ref connection) => connection.clone(),
            None => return Err(RedisError::Connection)
        };
        let res: bool = Cmd::hexists(key, field)
            .query_async(&mut connection)
            .await?;
        Ok(res)
    }

    ///exists redis command
    pub async fn exists(&self, key: &str) -> Result<bool> {
        let mut connection = match self.connection{
            Some(ref connection) => connection.clone(),
            None => return Err(RedisError::Connection)
        };
        let res: bool = Cmd::exists(key).query_async(&mut connection).await?;
        Ok(res)
    }

    pub async fn ping(&self) -> Result<()> {
        let mut connection = match self.connection{
            Some(ref connection) => connection.clone(),
            None => return Err(RedisError::Connection)
        };
        redis::cmd("PING").query_async(&mut connection).await?;
        Ok(())
    }
}

impl Debug for Client{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
       f.debug_struct("Client")
           .field("client", &self.client)
           .finish_non_exhaustive()
    }
}
