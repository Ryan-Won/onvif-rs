use crate::soap::client::Credentials;
use reqwest::{RequestBuilder, Response};
use std::fmt::{Debug, Formatter};
use thiserror::Error;
use url::Url;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid state")]
    InvalidState,
    #[error("No credentials")]
    NoCredentials,
    #[error("Digest {0}")]
    Digest(String),
}

pub struct Digest {
    creds: Option<Credentials>,
    uri: Url,
    state: State,
}

enum State {
    Default,
    Got401(reqwest::Response),
    Got401Twice,
}

impl Digest {
    pub fn new(uri: &Url, creds: &Option<Credentials>) -> Self {
        Self {
            creds: creds.clone(),
            uri: uri.clone(),
            state: State::Default,
        }
    }
}

impl Digest {
    pub fn set_401(&mut self, response: Response) {
        match self.state {
            State::Default => self.state = State::Got401(response),
            State::Got401(_) => self.state = State::Got401Twice,
            State::Got401Twice => {}
        }
    }

    pub fn is_failed(&self) -> bool {
        matches!(self.state, State::Got401Twice)
    }

    pub fn add_headers(&self, mut request: RequestBuilder) -> Result<RequestBuilder, Error> {
        match &self.state {
            State::Default => Ok(request),
            State::Got401(response) => {
                let creds = self.creds.as_ref().ok_or(Error::NoCredentials)?;

                request = request.header(
                    reqwest::header::AUTHORIZATION,
                    digest_auth(response, creds, &self.uri)?,
                );

                Ok(request)
            }
            State::Got401Twice => Err(Error::InvalidState),
        }
    }
}

fn digest_auth(res: &reqwest::Response, creds: &Credentials, url: &Url) -> Result<String, Error> {
    let www_auth_headers = res.headers().get_all(reqwest::header::WWW_AUTHENTICATE);

    let mut www_authenticate = None;

    for method in ["algorithm=sha", "algorithm=md5", "digest"] {
        for www_auth in www_auth_headers.iter() {
            let header_str = www_auth
                .to_str()
                .map_err(|e| Error::Digest(e.to_string()))?;
            if header_str.to_ascii_lowercase().contains(method) {
                www_authenticate = Some(header_str);
                break;
            }
        }
        if www_authenticate.is_some() {
            break;
        }
    }

    let www_authenticate = www_authenticate.ok_or(Error::Digest(
        "No www-authenticate digest header".to_string(),
    ))?;

    let mut context = digest_auth::AuthContext::new(&creds.username, &creds.password, url.path());

    context.method = digest_auth::HttpMethod::POST;

    Ok(digest_auth::parse(www_authenticate)
        .map_err(|e| Error::Digest(e.to_string()))?
        .respond(&context)
        .map_err(|e| Error::Digest(e.to_string()))?
        .to_string())
}

impl Debug for Digest {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Digest")
            .field("creds", &self.creds)
            .field("state", &self.state)
            .finish()
    }
}

impl Debug for State {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            State::Default => "FirstRequest",
            State::Got401(_) => "Got401",
            State::Got401Twice => "Got401Twice",
        })
    }
}
