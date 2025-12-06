use crate::config::StreamSection;

#[derive(Debug, Clone)]
pub struct Route {
    pub room_id: String,
    pub template: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Router {
    streams: Vec<StreamSection>,
    default_room: Option<String>,
}

impl Router {
    pub fn new(streams: Vec<StreamSection>, default_room: Option<String>) -> Self {
        Self { streams, default_room }
    }

    pub fn routes_for(&self, app_id: i64) -> Vec<Route> {
        let mut routes = Vec::new();
        for stream in &self.streams {
            if stream.apps.contains(&app_id) {
                for room in &stream.rooms {
                    routes.push(Route {
                        room_id: room.clone(),
                        template: stream.template_path.clone(),
                    });
                }
            }
        }
        if routes.is_empty() {
            if let Some(room) = &self.default_room {
                routes.push(Route {
                    room_id: room.clone(),
                    template: None,
                });
            }
        }
        // dedupe
        let mut uniq = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for r in routes {
            let key = (r.room_id.clone(), r.template.clone());
            if seen.insert(key) {
                uniq.push(r);
            }
        }
        uniq
    }
}
