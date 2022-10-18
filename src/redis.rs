use std::{env, fmt::Debug};

use log::{warn, debug};
use redis::{aio::Connection, aio::ConnectionManager, Cmd};
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RedisError {
    #[error("redis error: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("Not yet connected to the redis server.")]
    Connection,
    #[error("provided redis user without password")]
    NoPassword,
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
        self.password = env::var(prefix + "_REDIS_PASSWORD").ok().or_else(|| {
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
pub struct Client {
    client: redis::Client,
    pub connection: Option<ConnectionManager>,
}

///constuct the uri form the addr, user and password
fn construc_uri(addr: &str, user: &Option<String>, password: &Option<String>) -> Result<String> {
    let mut url = String::from("redis://");
    match password {
        Some(ref password) => {
            if let Some(ref user) = user {
                url += user as &str;
            }
            url = url + ":" + password as &str + "@";
        }
        None => {
            if user.is_some() {
                return Err(RedisError::NoPassword);
            }
        }
    }
    url += addr as &str;
    Ok(url)
}

impl Client {
    ///create a new client form the redis config
    pub fn new(config: &Redis) -> Result<Self> {
        let url = construc_uri(&config.addr, &config.user, &config.password)?;
        let info = redis::Client::open(url)?;
        let client = Client {
            client: info,
            connection: None,
        };
        Ok(client)
    }
    ///Return a simple connection , this connection is not managed,
    pub async fn get_simple_connection(&self) -> Result<Connection> {
        let conection = self.client.get_tokio_connection().await?;
        Ok(conection)
    }
    ///Connect the client to the redis server, it will try to reconnect
    ///automatically if an error is encountered.
    pub async fn connect(&mut self) -> Result<&mut Self> {
        let conection = self.client.get_tokio_connection_manager().await?;
        self.connection = Some(conection);
        Ok(self)
    }
    ///hset redis command
    pub async fn hset(&self, key: &str, field: &str, value: &str) -> Result<()> {
        let mut connection = match self.connection {
            Some(ref connection) => connection.clone(),
            None => return Err(RedisError::Connection),
        };
        Cmd::hset_nx(key, field, value)
            .query_async(&mut connection)
            .await?;
        Ok(())
    }

    ///hexists redis command
    pub async fn hexists(&self, key: &str, field: &str) -> Result<bool> {
        let mut connection = match self.connection {
            Some(ref connection) => connection.clone(),
            None => return Err(RedisError::Connection),
        };
        let res: bool = Cmd::hexists(key, field)
            .query_async(&mut connection)
            .await?;
        Ok(res)
    }

    ///exists redis command
    pub async fn exists(&self, key: &str) -> Result<bool> {
        let mut connection = match self.connection {
            Some(ref connection) => connection.clone(),
            None => return Err(RedisError::Connection),
        };
        let res: bool = Cmd::exists(key).query_async(&mut connection).await?;
        Ok(res)
    }

    pub async fn ping(&self) -> Result<()> {
        let mut connection = match self.connection {
            Some(ref connection) => connection.clone(),
            None => return Err(RedisError::Connection),
        };
        redis::cmd("PING").query_async(&mut connection).await?;
        Ok(())
    }
}

impl Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client")
            .field("client", &self.client)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod test_redis {
    use super::construc_uri;

    #[test]
    fn test_construct_uri_full() {
        let addr = "test.test:8080";
        let user = Some("toto".to_string());
        let password = Some("tata64".to_string());
        let expected = "redis://toto:tata64@test.test:8080";
        let res = construc_uri(addr, &user, &password).unwrap();
        assert_eq!(res, expected);
    }

    #[test]
    fn test_construct_uri_no_user() {
        let addr = "test.test:8080";
        let user = None;
        let password = Some("tata64".to_string());
        let expected = "redis://:tata64@test.test:8080";
        let res = construc_uri(addr, &user, &password).unwrap();
        assert_eq!(res, expected);
    }

    #[test]
    fn test_construct_uri_min() {
        let addr = "test.test:8080";
        let user = None;
        let password = None;
        let expected = "redis://test.test:8080";
        let res = construc_uri(addr, &user, &password).unwrap();
        assert_eq!(res, expected);
    }

    #[test]
    #[should_panic]
    fn test_construct_uri_invalid() {
        let addr = "test.test:8080";
        let user = Some("toto".to_string());
        let password = None;
        construc_uri(addr, &user, &password).unwrap();
    }
}
