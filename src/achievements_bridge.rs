//! Pont local entre un jeu en cours d'exécution et Quark, pour les succès.
//!
//! Certains jeux (ou leurs scripts) connaissent leurs propres succès mais ne
//! peuvent pas les écrire chez Epic ou GOG : Quark écoute donc sur la boucle
//! locale une petite API HTTP, avec un jeton propre à la session. L'adresse et
//! le jeton sont transmis au jeu par variables d'environnement au lancement :
//!
//! ```text
//! QUARK_ACHIEVEMENTS_URL=http://127.0.0.1:41234/achievements
//! QUARK_ACHIEVEMENTS_TOKEN=<jeton de session>
//! ```
//!
//! Rien n'écoute hors de `127.0.0.1`, et le jeton change à chaque lancement de
//! Quark.

use crate::providers::SessionUnlock;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Sender;
use std::time::Duration;

pub const ENV_URL: &str = "QUARK_ACHIEVEMENTS_URL";
pub const ENV_TOKEN: &str = "QUARK_ACHIEVEMENTS_TOKEN";

/// Taille maximale d'une requête : un succès tient en quelques centaines
/// d'octets, le plafond évite qu'un client local fasse grossir la mémoire.
const MAX_REQUEST: usize = 16 * 1024;

pub struct Bridge {
    url: String,
    token: String,
}

impl Bridge {
    pub fn url(&self) -> &str {
        &self.url
    }
    pub fn token(&self) -> &str {
        &self.token
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// Identifiant Quark du jeu, tel que transmis au lancement.
    pub game: String,
    pub unlock: SessionUnlock,
}

/// Démarre l'écoute sur un port libre de la boucle locale.
pub fn start(
    events: Sender<Report>,
    wake: impl Fn() + Send + Sync + 'static,
) -> Result<Bridge, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|_| "impossible d'ouvrir le pont des succès sur la boucle locale".to_string())?;
    let addr = listener
        .local_addr()
        .map_err(|_| "pont des succès sans adresse locale".to_string())?;
    let token = random_token()?;
    let url = format!("http://{addr}/achievements");
    let expected = token.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let _ = handle(&mut stream, &expected, &events);
            wake();
        }
    });
    Ok(Bridge { url, token })
}

fn random_token() -> Result<String, String> {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "impossible de générer le jeton du pont des succès".to_string())?;
    let mut token = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(token, "{byte:02x}");
    }
    Ok(token)
}

fn header_value<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

fn authorized(head: &str, token: &str) -> bool {
    let Some(value) = header_value(head, "authorization") else {
        return false;
    };
    let Some(candidate) = value.strip_prefix("Bearer ") else {
        return false;
    };
    candidate.as_bytes().len() == token.as_bytes().len()
        && candidate
            .as_bytes()
            .iter()
            .zip(token.as_bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

/// Réponse HTTP minimale, corps JSON et fermeture de la connexion.
fn respond(stream: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes())
}

fn handle(stream: &mut TcpStream, token: &str, events: &Sender<Report>) -> std::io::Result<()> {
    let mut raw = Vec::new();
    let mut chunk = [0u8; 2048];
    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        raw.extend_from_slice(&chunk[..read]);
        if raw.len() > MAX_REQUEST {
            return respond(
                stream,
                "413 Payload Too Large",
                r#"{"ok":false,"error":"requête trop grande"}"#,
            );
        }
        if raw.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    // Le corps peut arriver après les en-têtes : on complète jusqu'à la
    // longueur annoncée, dans la limite du plafond.
    if let Some(split) = raw.windows(4).position(|window| window == b"\r\n\r\n") {
        let head_end = split + 4;
        let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
        let expected = header_value(&head, "content-length")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        while raw.len() < head_end + expected && raw.len() <= MAX_REQUEST {
            let read = stream.read(&mut chunk)?;
            if read == 0 {
                break;
            }
            raw.extend_from_slice(&chunk[..read]);
        }
    }
    let text = String::from_utf8_lossy(&raw).into_owned();
    let Some((head, body)) = text.split_once("\r\n\r\n") else {
        return respond(
            stream,
            "400 Bad Request",
            r#"{"ok":false,"error":"requête HTTP incomplète"}"#,
        );
    };
    let request_line = head.lines().next().unwrap_or_default();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let path = parts.next().unwrap_or_default();
    if !matches!(path, "/achievements" | "/v1/achievements") {
        return respond(
            stream,
            "404 Not Found",
            r#"{"ok":false,"error":"chemin inconnu"}"#,
        );
    }
    if !authorized(head, token) {
        return respond(
            stream,
            "401 Unauthorized",
            r#"{"ok":false,"error":"jeton de session invalide"}"#,
        );
    }
    if method == "GET" {
        return respond(
            stream,
            "200 OK",
            r#"{"ok":true,"endpoint":"achievements","format":"QUARK_ACHIEVEMENTS_URL"}"#,
        );
    }
    if method != "POST" {
        return respond(
            stream,
            "405 Method Not Allowed",
            r#"{"ok":false,"error":"utilise POST"}"#,
        );
    }
    let Some(report) = parse_report(body) else {
        return respond(
            stream,
            "400 Bad Request",
            r#"{"ok":false,"error":"JSON attendu : {\"game\":\"…\",\"achievement\":\"CLÉ\"}"}"#,
        );
    };
    let name = if report.unlock.name.trim().is_empty() {
        report.unlock.key.clone()
    } else {
        report.unlock.name.clone()
    };
    if events.send(report).is_err() {
        return respond(
            stream,
            "503 Service Unavailable",
            r#"{"ok":false,"error":"Quark n'écoute plus"}"#,
        );
    }
    respond(
        stream,
        "200 OK",
        &format!(
            "{{\"ok\":true,\"recorded\":{}}}",
            serde_json::to_string(&name).unwrap_or_else(|_| "\"succès\"".to_string())
        ),
    )
}

fn parse_report(body: &str) -> Option<Report> {
    #[derive(serde::Deserialize)]
    struct Payload {
        game: String,
        #[serde(default, alias = "achievement", alias = "key")]
        achievement: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        description: String,
        /// Seul un déblocage est accepté : un jeu ne peut pas verrouiller un
        /// succès déjà obtenu.
        #[serde(default = "default_unlocked")]
        unlocked: bool,
        #[serde(default)]
        at: i64,
    }
    fn default_unlocked() -> bool {
        true
    }
    let payload: Payload = serde_json::from_str(body).ok()?;
    let game = payload.game.trim();
    if game.is_empty() || game.len() > 200 {
        return None;
    }
    let key = payload.achievement.trim();
    let name = payload.name.trim();
    if (key.is_empty() && name.is_empty()) || key.len() > 200 || name.len() > 200 {
        return None;
    }
    if payload.description.len() > 600 || !payload.unlocked {
        return None;
    }
    let now = crate::config::now_ts();
    Some(Report {
        game: game.to_string(),
        unlock: SessionUnlock {
            key: if key.is_empty() {
                name.to_string()
            } else {
                key.to_string()
            },
            name: name.to_string(),
            description: payload.description.trim().to_string(),
            at: if payload.at > 0 { payload.at } else { now },
            synced: false,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;
    use std::sync::mpsc::channel;

    fn call(bridge: &Bridge, body: &str, token: &str) -> String {
        let addr = bridge
            .url()
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap();
        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let request = format!(
            "POST /achievements HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    #[test]
    fn un_succes_authentifie_est_transmis_et_un_jeton_invalide_refuse() {
        let (tx, rx) = channel();
        let bridge = start(tx, || {}).unwrap();
        let body = r#"{"game":"quark-1","achievement":"ACH_FIRST","name":"Premier pas","description":"Fini le niveau 1","at":1700000000}"#;
        let rejected = call(&bridge, body, "mauvais-jeton");
        assert!(rejected.starts_with("HTTP/1.1 401"), "{rejected}");
        let response = call(&bridge, body, bridge.token());
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        let report = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(report.game, "quark-1");
        assert_eq!(report.unlock.key, "ACH_FIRST");
        assert_eq!(report.unlock.name, "Premier pas");
        assert_eq!(report.unlock.at, 1_700_000_000);
        assert!(!report.unlock.synced);
    }

    #[test]
    fn un_verrouillage_ou_un_json_incomplet_est_refuse() {
        let (tx, rx) = channel();
        let bridge = start(tx, || {}).unwrap();
        let locked = call(
            &bridge,
            r#"{"game":"quark-1","achievement":"A","unlocked":false}"#,
            bridge.token(),
        );
        assert!(locked.starts_with("HTTP/1.1 400"), "{locked}");
        let empty = call(&bridge, r#"{"game":"","achievement":""}"#, bridge.token());
        assert!(empty.starts_with("HTTP/1.1 400"), "{empty}");
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn le_jeton_change_a_chaque_demarrage() {
        let (tx, _rx) = channel();
        let first = start(tx, || {}).unwrap();
        let (tx, _rx) = channel();
        let second = start(tx, || {}).unwrap();
        assert_ne!(first.token(), second.token());
        assert_ne!(first.url(), second.url());
        assert!(first.url().starts_with("http://127.0.0.1:"));
    }
}
