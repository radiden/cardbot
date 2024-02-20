use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use config::Config;
use http::header::HeaderMap;
use poise::serenity_prelude::{self as serenity};
use regex::Regex;
use serde::Serialize;
use sqlx::Row;
use std::error;
use std::future::IntoFuture;

#[derive(Clone)]
struct AxumState {
    database: sqlx::SqlitePool,
    api_password: String,
}
struct Data {
    database: sqlx::SqlitePool,
}
type Error = Box<dyn std::error::Error + Send + Sync>;
type Context<'a> = poise::Context<'a, Data, Error>;

#[derive(sqlx::FromRow, Serialize)]
struct Card {
    id: String,
    #[serde(rename(serialize = "name"))]
    username: String,
}

#[derive(Serialize)]
struct CardList {
    pages: Vec<Page>,
}

#[derive(Serialize)]
struct Page {
    cards: Vec<Card>,
}

// adds a card to db
#[poise::command(
    slash_command,
    name_localized("pl", "dodaj_karte"),
    description_localized("pl", "Dodaje karte do naszej bazy kart"),
    description_localized("en-GB", "Adds a card to our database"),
    description_localized("en-US", "Adds a card to our database"),
    ephemeral
)]
async fn add_card(
    ctx: Context<'_>,
    #[description = "Card ID"]
    #[name_localized("pl", "karta")]
    #[description_localized("pl", "ID Karty")]
    card: String,
) -> Result<(), Error> {
    let mut card_upper = card.to_uppercase();
    card_upper.retain(|c| !c.is_whitespace());
    let re = Regex::new(r"^[0-9A-F]{16}$").unwrap();
    let valid = re.is_match(card_upper.as_str());

    let user_id = ctx.author().id.to_string();

    match valid {
        true => {
            let locale = ctx.locale().unwrap_or("en");

            let user_card =
                sqlx::query("SELECT (id IS null) as id_is_null FROM cards WHERE owner_id = ?")
                    .bind(user_id.clone())
                    .fetch_one(&ctx.data().database)
                    .await?;

            let exists = !user_card.get::<bool, _>(0);

            if exists {
                let update = sqlx::query!(
                    "UPDATE cards SET id = ? WHERE owner_id = ?",
                    card_upper,
                    user_id
                )
                .execute(&ctx.data().database)
                .await;

                match update {
                    Ok(_) => {
                        if locale == "pl" {
                            ctx.say("Zaktualizowano kartę.").await?;
                        } else {
                            ctx.say("Card updated.").await?;
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        return Err(Box::new(e));
                    }
                }
            }

            let insert = sqlx::query!(
                "INSERT INTO cards (id, owner_id, username) VALUES (?, ?, ?)",
                card_upper,
                user_id,
                ctx.author().name
            )
            .execute(&ctx.data().database)
            .await;

            if insert.is_err() {
                if locale == "pl" {
                    ctx.say("Nie udało się dodać karty do bazy.").await?;
                } else {
                    ctx.say("Couldn't add the card to the database.").await?;
                }
                return Err(Box::new(insert.unwrap_err()));
            }

            if locale == "pl" {
                ctx.say("Dodano karte {card} do bazy.").await?;
            } else {
                ctx.say("Added {card} to the database.").await?;
            }
        }
        false => {
            let locale = ctx.locale().unwrap_or("en");
            if locale == "pl" {
                ctx.say("Nieprawidłowe ID karty.").await?;
            } else {
                ctx.say("Invalid card ID.").await?;
            }
        }
    }

    Ok(())
}

#[poise::command(
    slash_command,
    name_localized("pl", "moja_karta"),
    description_localized("pl", "Sprawdza czy karta jest dodana do naszej bazy"),
    description_localized("en-GB", "Checks if a card is in our database"),
    description_localized("en-US", "Checks if a card is in our database"),
    ephemeral
)]
async fn my_card(ctx: Context<'_>) -> Result<(), Error> {
    let user_id = ctx.author().id.to_string();

    let user_card = sqlx::query_as::<_, Card>(
        "SELECT cast(id as text) as id, username FROM cards WHERE owner_id = ?",
    )
    .bind(user_id.clone())
    .fetch_one(&ctx.data().database)
    .await
    .map_err(|_| "No card in database.")?;

    ctx.say(format!("{} - {}", user_card.username, user_card.id))
        .await?;

    Ok(())
}

async fn cards(
    headers: HeaderMap,
    State(state): State<AxumState>,
) -> Result<(StatusCode, Json<CardList>), String> {
    if headers
        .get("password")
        .and_then(|value| value.to_str().ok())
        != Some(&state.api_password)
    {
        return Err("invalid password".to_owned());
    }

    let cards = sqlx::query_as::<_, Card>("SELECT cast(id as text) as id, username FROM cards")
        .fetch_all(&state.database)
        .await
        .map_err(|e| format!("could not get cards from database: {e:?}"))?;

    Ok((
        StatusCode::OK,
        Json(CardList {
            pages: vec![Page { cards }],
        }),
    ))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn error::Error>> {
    let settings = Config::builder()
        .add_source(config::File::with_name("config.toml").required(false))
        .add_source(config::Environment::with_prefix("CARDBOT"))
        .build()
        .expect("expected to be able to load the config");

    let db_file = settings.get::<String>("db_file").expect("need db_file");

    let api_password = settings
        .get::<String>("api_password")
        .expect("need api_password");

    let database = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&db_file)
                .create_if_missing(true),
        )
        .await
        .expect("can't connect to database");

    sqlx::migrate!("./migrations")
        .run(&database)
        .await
        .expect("can't run migrations");

    let token = settings.get::<String>("bot_token").expect("need token");

    let serenity_db = database.clone();

    let intents = serenity::GatewayIntents::non_privileged();
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![add_card(), my_card()],
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                Ok(Data {
                    database: serenity_db,
                })
            })
        })
        .build();

    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .await?;

    let axum_serve = axum::serve(
        tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap(),
        Router::new()
            .route("/cards.json", get(cards))
            .with_state(AxumState {
                database,
                api_password,
            }),
    );

    let (res_axum, res_serenity) = tokio::join!(axum_serve.into_future(), client.start());

    res_axum?;
    res_serenity?;

    Ok(())
}
