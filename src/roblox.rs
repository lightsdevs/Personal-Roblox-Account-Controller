use crate::store::Account;
use reqwest::{
    blocking::{Client, RequestBuilder, Response},
    header::{self, HeaderValue},
    redirect::Policy,
};
use serde::Deserialize;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

struct Session {
    client: Client,
    cookie: HeaderValue,
}
impl Session {
    fn new(cookie: &str) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(Policy::none())
            .build()
            .map_err(|_| "Could not initialize networking.")?;
        let mut cookie = HeaderValue::from_str(&format!(".ROBLOSECURITY={cookie}"))
            .map_err(|_| "Invalid saved session. Sign in again.")?;
        cookie.set_sensitive(true);
        Ok(Self { client, cookie })
    }
    fn get(&self, url: &str) -> RequestBuilder {
        self.client
            .get(url)
            .header(header::COOKIE, self.cookie.clone())
    }
    fn post(&self, url: &str) -> RequestBuilder {
        self.client
            .post(url)
            .header(header::COOKIE, self.cookie.clone())
    }
    fn ticket(&self) -> Result<String, String> {
        const ENDPOINT: &str = "https://auth.roblox.com/v1/authentication-ticket/";
        let challenge = send(self.post(ENDPOINT).header(
            header::REFERER,
            "https://www.roblox.com/games/3398014311/Restaurant-Tycoon-2",
        ))?;
        let mut csrf = challenge
            .headers()
            .get("x-csrf-token")
            .cloned()
            .ok_or("Could not refresh the session. Sign in again.")?;
        csrf.set_sensitive(true);
        let response = checked(
            self.post(ENDPOINT)
                .header("x-csrf-token", csrf)
                .header(header::REFERER, "https://www.roblox.com/")
                .header(header::CONTENT_TYPE, "application/json"),
        )?;
        response
            .headers()
            .get("rbx-authentication-ticket")
            .and_then(|h| h.to_str().ok())
            .filter(|s| {
                !s.is_empty()
                    && s.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_=/.".contains(&b))
            })
            .map(str::to_owned)
            .ok_or("Roblox did not provide a usable launch ticket.".into())
    }
}
fn send(request: RequestBuilder) -> Result<Response, String> {
    request
        .send()
        .map_err(|_| "Roblox request failed (network or timeout).".into())
}
fn checked(request: RequestBuilder) -> Result<Response, String> {
    let response = send(request)?;
    if !response.status().is_success() {
        return Err(format!(
            "Roblox returned HTTP {}. If your session expired, sign in again.",
            response.status().as_u16()
        ));
    }
    Ok(response)
}
#[derive(Deserialize)]
struct Identity {
    #[serde(rename = "Name")]
    username: String,
    #[serde(rename = "UserId")]
    id: u64,
}
pub fn validate(cookie: String) -> Result<Account, String> {
    let session = Session::new(&cookie)?;
    let identity: Identity = checked(session.get("https://www.roblox.com/my/account/json"))?
        .json()
        .map_err(|_| "Roblox did not return a valid account identity.")?;
    if identity.id == 0 || identity.username.trim().is_empty() {
        return Err("Roblox returned an empty account identity.".into());
    }
    Ok(Account {
        username: identity.username,
        cookie,
        description: String::new(),
    })
}
#[derive(Clone)]
pub enum Target {
    Place(u64),
    User(String),
}
#[derive(Deserialize)]
struct Place {
    #[serde(rename = "placeId")]
    id: u64,
}
#[derive(Deserialize)]
struct Users {
    data: Vec<User>,
}
#[derive(Deserialize)]
struct User {
    id: u64,
}
fn launcher_url(follow: bool, id: u64, tracker: &str, attempt: u64) -> String {
    let mut url =
        url::Url::parse("https://www.roblox.com/Game/PlaceLauncher.ashx").expect("constant URL");
    {
        let mut query = url.query_pairs_mut();
        query.append_pair(
            "request",
            if follow {
                "RequestFollowUser"
            } else {
                "RequestGame"
            },
        );
        query.append_pair("browserTrackerId", tracker);
        if follow {
            query
                .append_pair("userId", &id.to_string())
                .append_pair("joinAttemptId", &attempt.to_string())
                .append_pair("joinAttemptOrigin", "peopleListInHomePage");
        } else {
            query
                .append_pair("placeId", &id.to_string())
                .append_pair("isPlayTogetherGame", "false");
        }
    }
    url.to_string()
}
fn protocol(ticket: &str, launcher: &str, tracker: &str, now: u128) -> String {
    let encoded: String = url::form_urlencoded::byte_serialize(launcher.as_bytes()).collect();
    format!("roblox-player:1+launchmode:play+gameinfo:{ticket}+launchtime:{now}+placelauncherurl:{encoded}+browsertrackerid:{tracker}+robloxLocale:en_us+gameLocale:en_us+channel:+LaunchExp:InApp")
}
pub fn join(account: Account, target: Target) -> Result<(), String> {
    let session = Session::new(&account.cookie)?;
    let ticket = session.ticket()?;
    let (follow, id) = match target {
        Target::Place(id) => {
            let places: Vec<Place> = checked(
                session
                    .get("https://games.roblox.com/v1/games/multiget-place-details")
                    .query(&[("placeIds", id)]),
            )?
            .json()
            .map_err(|_| "Roblox returned an invalid place response.")?;
            if !places.iter().any(|p| p.id == id) {
                return Err("Place not found.".into());
            }
            (false, id)
        }
        Target::User(username) => {
            let users: Users = checked(
                session
                    .post("https://users.roblox.com/v1/usernames/users")
                    .json(
                        &serde_json::json!({"usernames": [username], "excludeBannedUsers": true}),
                    ),
            )?
            .json()
            .map_err(|_| "Roblox returned an invalid username response.")?;
            (
                true,
                users
                    .data
                    .first()
                    .filter(|u| u.id > 0)
                    .ok_or("Username not found.")?
                    .id,
            )
        }
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "System clock is invalid.")?;
    let tracker = format!(
        "{}{}",
        100000 + now.as_nanos() % 1655001,
        100000 + now.as_micros() % 800001
    );
    let launcher = launcher_url(
        follow,
        id,
        &tracker,
        (100000 + (now.as_nanos() / 7) % 1655001) as u64,
    );
    crate::platform::launch(&protocol(&ticket, &launcher, &tracker, now.as_millis()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_contract_matches_python() {
        for follow in [false, true] {
            let launcher = launcher_url(follow, 42, "123456654321", 789012);
            let url = url::Url::parse(&launcher).unwrap();
            let pairs: std::collections::HashMap<_, _> = url.query_pairs().collect();
            assert_eq!(
                pairs["request"],
                if follow {
                    "RequestFollowUser"
                } else {
                    "RequestGame"
                }
            );
            assert_eq!(pairs[if follow { "userId" } else { "placeId" }], "42");
            let uri = protocol("synthetic-ticket", &launcher, "123456654321", 123);
            assert!(uri.starts_with(
                "roblox-player:1+launchmode:play+gameinfo:synthetic-ticket+launchtime:123+"
            ));
            let encoded = uri
                .split("placelauncherurl:")
                .nth(1)
                .unwrap()
                .split('+')
                .next()
                .unwrap();
            let decoded = url::form_urlencoded::parse(format!("url={encoded}").as_bytes())
                .next()
                .unwrap()
                .1
                .into_owned();
            assert_eq!(decoded, launcher);
            assert!(uri.ends_with("+robloxLocale:en_us+gameLocale:en_us+channel:+LaunchExp:InApp"));
        }
    }
}
