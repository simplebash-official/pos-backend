use mongodb::{
    Client, Database,
    options::{ClientOptions, ResolverConfig},
};

pub async fn connect(uri: &str, db_name: &str) -> mongodb::error::Result<Database> {
    let mut last_err = None;
    for attempt in 0..5 {
        if attempt > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(250 * attempt)).await;
        }

        let parse_res = ClientOptions::parse(uri).await;
        let options_res = match parse_res {
            Ok(opts) => Ok(opts),
            Err(_) => match ClientOptions::parse(uri)
                .resolver_config(ResolverConfig::cloudflare())
                .await
            {
                Ok(opts) => Ok(opts),
                Err(_) => {
                    ClientOptions::parse(uri)
                        .resolver_config(ResolverConfig::google())
                        .await
                }
            },
        };

        let mut options = match options_res {
            Ok(opts) => opts,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };
        options.app_name = Some("myrologic-pos-backend".to_string());

        let client = match Client::with_options(options) {
            Ok(c) => c,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };

        match client
            .database(db_name)
            .run_command(mongodb::bson::doc! { "ping": 1 })
            .await
        {
            Ok(_) => return Ok(client.database(db_name)),
            Err(e) => {
                last_err = Some(e);
            }
        }
    }

    Err(last_err.expect("connect failed after retries"))
}
