// Сетевой слой для wasm: HTTP через браузерный fetch (window.fetch).
//
// API повторяет прежний reqwest-билдер, чтобы модули diary/login/marks не
// пришлось переписывать: RequestBuilder (.get/.post/.header/.timeout/.json/.send)
// и Response (.status/.headers/.bytes/.text), Error::is_timeout().
//
// ВАЖНО: браузер применяет CORS — сервер дневника должен отвечать заголовками
// Access-Control-Allow-*, либо запросы надо пускать через прокси. Если
// страница открыта как `...?proxy=https://my-proxy.example/`, все запросы к
// mp2.obr57.ru идут на `<proxy><оригинальный путь>`.

use std::sync::OnceLock;

use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{window, AbortController, Headers, RequestInit};
use web_sys::Response as WebResponse;

use std::time::Duration;

const DEFAULT_ORIGIN: &str = "https://mp2.obr57.ru";

// Прокси из query-параметра ?proxy= (если задан) — иначе прямой origin.
pub(crate) fn api_origin() -> &'static String {
    static ORIGIN: OnceLock<String> = OnceLock::new();
    ORIGIN.get_or_init(|| {
        if let Some(loc) = window().map(|w| w.location()) {
            if let Ok(search) = loc.search() {
                if let Ok(params) = web_sys::UrlSearchParams::new_with_str(&search) {
                    if let Some(p) = params.get("proxy") {
                        let p = p.trim_end_matches('/').to_string();
                        if !p.is_empty() {
                            log::info!("Используется прокси: {}", p);
                            return p;
                        }
                    }
                }
            }
        }
        DEFAULT_ORIGIN.to_string()
    })
}

// Полный URL: относительный путь ("https://..." заменяется на origin/путь
// с учётом прокси).
pub(crate) fn api_url(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("https://mp2.obr57.ru") {
        format!("{}{}", api_origin(), rest)
    } else {
        path.to_string()
    }
}

// ---------------------------------------------------------------
//  Ошибки
// ---------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq)]
enum ErrKind {
    Timeout,
    Network,
}

#[derive(Debug, Clone)]
pub struct Error {
    kind: ErrKind,
    message: String,
}

impl Error {
    // Таймаут отличаем от прочих сетевых сбоев (как reqwest::is_timeout):
    // почти всегда это иностранный IP/VPN — сервер дневника не отвечает.
    pub fn is_timeout(&self) -> bool {
        self.kind == ErrKind::Timeout
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for Error {}

// ---------------------------------------------------------------
//  Статус ответа
// ---------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusCode(u16);

impl StatusCode {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.0)
    }
}

impl std::fmt::Display for StatusCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Заголовки не нужны вызывающему коду (только логирование), отдаём заглушку.
#[derive(Debug, Clone, Default)]
pub struct HeaderMap;

// ---------------------------------------------------------------
//  Response
// ---------------------------------------------------------------
pub struct Response {
    status: StatusCode,
    body: Vec<u8>,
}

impl Response {
    pub fn status(&self) -> StatusCode {
        self.status
    }
    pub fn headers(&self) -> HeaderMap {
        HeaderMap
    }
    pub async fn bytes(self) -> Result<Vec<u8>, Error> {
        Ok(self.body)
    }
    pub async fn text(self) -> Result<String, Error> {
        String::from_utf8(self.body)
            .map_err(|e| Error { kind: ErrKind::Network, message: format!("не UTF-8 тело ответа: {e}") })
    }
}

// ---------------------------------------------------------------
//  Request builder
// ---------------------------------------------------------------
pub struct RequestBuilder {
    method: &'static str,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
    timeout: Duration,
}

impl RequestBuilder {
    fn new(method: &'static str, url: &str) -> Self {
        Self {
            method,
            url: api_url(url),
            headers: Vec::new(),
            body: None,
            timeout: Duration::from_secs(15),
        }
    }
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
    // На wasm настоящих таймаутов у fetch нет — используется AbortController.
    pub fn timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }
    pub fn json<T: serde::Serialize>(mut self, value: &T) -> Self {
        match serde_json::to_vec(value) {
            Ok(bytes) => {
                self.body = Some(bytes);
                self.headers
                    .push(("Content-Type".into(), "application/json".into()));
            }
            Err(e) => log::error!("Не удалось сериализовать тело запроса: {e:?}"),
        }
        self
    }

    pub async fn send(self) -> Result<Response, Error> {
        send_fetch(self).await
    }
}

pub fn get(url: &str) -> RequestBuilder {
    RequestBuilder::new("GET", url)
}

pub fn post(url: &str) -> RequestBuilder {
    RequestBuilder::new("POST", url)
}

// Объект-клиент (совместим с прежним client().post(...)/.get(...)).
pub struct Client;

impl Client {
    pub fn get(&self, url: &str) -> RequestBuilder {
        get(url)
    }
    pub fn post(&self, url: &str) -> RequestBuilder {
        post(url)
    }
}

pub fn client() -> Client {
    Client
}

async fn send_fetch(req: RequestBuilder) -> Result<Response, Error> {
    let win = window().ok_or_else(|| Error {
        kind: ErrKind::Network,
        message: "нет window".into(),
    })?;

    // Таймаут через AbortController (у fetch нет встроенного таймаута).
    let abort = AbortController::new().map_err(|_| Error {
        kind: ErrKind::Network,
        message: "AbortController недоступен".into(),
    })?;
    let timeout_ms = req.timeout.as_millis() as i32;
    let callback = {
        let controller = abort.clone();
        let closure = Closure::<dyn Fn()>::new(move || controller.abort());
        win.set_timeout_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            timeout_ms,
        )
        .ok()
        .map(|id| (id, closure))
    };

    let result = do_fetch(&win, &abort, &req).await;

    // Снимаем таймер, чтобы не держать closure и не абортить следующие запросы.
    if let Some((id, _)) = callback {
        win.clear_timeout_with_handle(id);
    }

    result
}

async fn do_fetch(
    win: &web_sys::Window,
    abort: &AbortController,
    req: &RequestBuilder,
) -> Result<Response, Error> {
    let timed_out = |err: &JsValue| -> bool {
        // При аборт fetch реджектится DOMException "AbortError".
        err.is_instance_of::<js_sys::Error>()
            && err.dyn_ref::<js_sys::Error>().unwrap().name() == "AbortError"
    };

    let headers = Headers::new().map_err(|_| Error {
        kind: ErrKind::Network,
        message: "Headers недоступен".into(),
    })?;
    for (k, v) in &req.headers {
        // Host/User-Agent управляются браузером и запрещены в fetch.
        if k.eq_ignore_ascii_case("host") || k.eq_ignore_ascii_case("user-agent") {
            continue;
        }
        let _ = headers.set(k, v);
    }

    let mut init = RequestInit::new();
    init.set_method(req.method);
    init.set_headers(&headers);
    init.set_signal(Some(&abort.signal()));
    if let Some(body) = &req.body {
        let arr = Uint8Array::from(body.as_slice());
        init.set_body(&arr.into());
    }

    let promise = win.fetch_with_str_and_init(&req.url, &init);
    let resp_val = match JsFuture::from(promise).await {
        Ok(v) => v,
        Err(e) => {
            return Err(Error {
                kind: if timed_out(&e) { ErrKind::Timeout } else { ErrKind::Network },
                message: format!("fetch не выполнен: {e:?}"),
            })
        }
    };

    let resp: WebResponse = resp_val
        .dyn_into()
        .map_err(|_| Error { kind: ErrKind::Network, message: "ответ не Response".into() })?;

    let status = StatusCode(resp.status());

    let buf_val = JsFuture::from(
        resp.array_buffer()
            .map_err(|_| Error { kind: ErrKind::Network, message: "arrayBuffer недоступен".into() })?,
    )
    .await
    .map_err(|e| Error {
        kind: if timed_out(&e) { ErrKind::Timeout } else { ErrKind::Network },
        message: format!("тело ответа не получено: {e:?}"),
    })?;

    let body = Uint8Array::new(&buf_val).to_vec();

    Ok(Response { status, body })
}

// ---------------------------------------------------------------
//  «Рантайм»: на wasm всё в одном потоке, spawn = spawn_local.
// ---------------------------------------------------------------
pub struct Runtime;

impl Runtime {
    pub fn spawn<F>(&self, fut: F)
    where
        F: std::future::Future<Output = ()> + 'static,
    {
        wasm_bindgen_futures::spawn_local(fut);
    }
}

pub fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| Runtime)
}
