// Сетевой протокол без сети: core только описывает запрос, платформа его
// выполняет (reqwest на мобильных, fetch в браузере) и возвращает ответ.

use serde::Serialize;

pub const API_BASE: &str = "https://mp2.obr57.ru";
pub const USER_AGENT: &str = "Dalvik/2.1.0 (Linux; U; Android 13)";
pub const X_REQUESTED_WITH: &str = "ru.integrics.orelschool";
pub const PDA_KEY: &str = "000xpda";

/// HTTP-запрос, готовый к исполнению адаптером.
#[derive(Clone, Debug)]
pub struct ApiRequest {
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout_secs: u64,
}

impl ApiRequest {
    pub fn post_json<T: Serialize>(url: &str, body: &T, timeout_secs: u64) -> Self {
        let body = serde_json::to_vec(body).unwrap_or_default();
        Self {
            method: "POST",
            url: url.to_string(),
            headers: vec![
                ("User-Agent".into(), USER_AGENT.into()),
                ("Content-Type".into(), "application/json".into()),
                ("X-Requested-With".into(), X_REQUESTED_WITH.into()),
            ],
            body: Some(body),
            timeout_secs,
        }
    }

    pub fn get(url: &str, timeout_secs: u64) -> Self {
        Self {
            method: "GET",
            url: url.to_string(),
            headers: vec![
                ("User-Agent".into(), "burmalda57-app".into()),
                ("Accept".into(), "application/vnd.github+json".into()),
            ],
            body: None,
            timeout_secs,
        }
    }
}

/// Ответ транспорта. Ошибки статуса разбирает core, а не адаптер.
#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Сбой на уровне соединения (до получения HTTP-ответа).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportError {
    Offline,
    Timeout,
}

pub type HttpResult = Result<HttpResponse, TransportError>;

/// Ошибка операции журнала — с понятным пользователю текстом.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    Offline,
    Timeout,
    Blocked,
    Unauthorized,
    Server(u16),
    BadData,
}

impl FetchError {
    pub fn message(&self) -> String {
        match self {
            FetchError::Offline => "Нет связи — проверьте сеть или VPN".to_string(),
            FetchError::Timeout => {
                "Сервер не отвечает — проверьте сеть или отключите VPN".to_string()
            }
            FetchError::Blocked => "Журнал не пускает".to_string(),
            FetchError::Unauthorized => "Сессия истекла — войдите заново".to_string(),
            FetchError::Server(code) => {
                format!("Сбой на сервере журнала (ошибка {})", code)
            }
            FetchError::BadData => "Журнал прислал непонятный ответ".to_string(),
        }
    }
}

impl From<TransportError> for FetchError {
    fn from(e: TransportError) -> Self {
        match e {
            TransportError::Offline => FetchError::Offline,
            TransportError::Timeout => FetchError::Timeout,
        }
    }
}

pub fn status_error(status: u16) -> Option<FetchError> {
    if (200..300).contains(&status) {
        return None;
    }
    Some(match status {
        401 | 403 => FetchError::Unauthorized,
        // 407/429/451 на практике = отказ по IP (VPN)
        407 | 429 | 451 => FetchError::Blocked,
        _ => FetchError::Server(status),
    })
}

/// Проверить статус и вернуть тело как строку.
pub fn body_text(resp: HttpResponse) -> Result<String, FetchError> {
    if let Some(e) = status_error(resp.status) {
        log::warn!("HTTP {} → блокировка/VPN?", resp.status);
        return Err(e);
    }
    Ok(String::from_utf8_lossy(&resp.body).to_string())
}

pub fn body_json<T: serde::de::DeserializeOwned>(resp: HttpResponse) -> Result<T, FetchError> {
    let text = body_text(resp)?;
    serde_json::from_str(&text).map_err(|e| {
        log::error!("Не удалось разобрать ответ журнала: {:?}", e);
        FetchError::BadData
    })
}

/// Выполнить «сырой» POST: статус + текст тела.
pub fn post_raw(result: HttpResult) -> Result<String, FetchError> {
    let resp = result.map_err(FetchError::from)?;
    log::info!("HTTP {} байт={}", resp.status, resp.body.len());
    body_text(resp)
}
