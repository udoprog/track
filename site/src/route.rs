use crate::manual::find;

/// What the address names. Routes live in the hash, since GitHub Pages has
/// no fallback for other paths: `#/` is the landing page, `#install` one of
/// its sections, `#/manual/<page>` a manual page and `#/manual/<page>#<id>`
/// a heading on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    Home {
        anchor: Option<String>,
    },
    Manual {
        page: String,
        anchor: Option<String>,
    },
}

impl Route {
    pub fn parse(hash: &str) -> Self {
        let hash = hash.strip_prefix('#').unwrap_or(hash);

        let Some(path) = hash.strip_prefix('/') else {
            let anchor = (!hash.is_empty()).then(|| hash.to_owned());
            return Route::Home { anchor };
        };

        let Some(rest) = path.strip_prefix("manual/") else {
            return Route::Home { anchor: None };
        };

        let (page, anchor) = match rest.split_once('#') {
            Some((page, anchor)) => (page, Some(anchor.to_owned())),
            None => (rest, None),
        };

        Route::Manual {
            page: page.to_owned(),
            anchor,
        }
    }

    pub fn anchor(&self) -> Option<&str> {
        match self {
            Route::Home { anchor } | Route::Manual { anchor, .. } => anchor.as_deref(),
        }
    }

    pub fn title(&self) -> String {
        match self {
            Route::Home { .. } => String::from("track: know what to watch tonight"),
            Route::Manual { page, .. } if page.is_empty() => String::from("The track manual"),
            Route::Manual { page, .. } => match find(page) {
                Some(page) => format!("{} · track manual", page.title),
                None => String::from("Not found · track manual"),
            },
        }
    }
}
