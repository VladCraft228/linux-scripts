//! Synchronous message boundary. Calls stay on Qt's GUI thread; no IPC or worker polling.
use crate::{
    model::{Config, Group, Model, Task},
    x11::{Backend, Rect},
};
use serde_json::{json, Value};
use std::path::PathBuf;
pub struct Engine {
    pub model: Model,
    pub backend: Backend,
    last_groups: Option<Vec<Group>>,
    last_tasks: Option<Vec<Task>>,
    last_config: Option<Config>,
}
impl Engine {
    fn new(v: &Value) -> Result<Self, String> {
        let config: Config =
            serde_json::from_value(v["config"].clone()).map_err(|e| e.to_string())?;
        let model = Model::new(
            config,
            PathBuf::from(v["importDir"].as_str().ok_or("Missing import directory")?),
            serde_json::from_value(v["dataDirs"].clone()).map_err(|e| e.to_string())?,
        );
        let backend = Backend::new().map_err(|e| e.to_string())?;
        Ok(Self {
            model,
            backend,
            last_groups: None,
            last_tasks: None,
            last_config: None,
        })
    }
    fn command(&mut self, v: &Value) -> Result<(Value, bool), String> {
        let b = &mut self.backend;
        let key = v["key"].as_str().unwrap_or("");
        let wid = v["wid"].as_u64().unwrap_or(0) as u32;
        let mut refresh = false;
        let mut result = Value::Null;
        let run = (|| -> Result<(), Box<dyn std::error::Error>> {
            match v["op"].as_str().unwrap_or("") {
                "attach" => {
                    b.attach(
                        wid,
                        v["scale"].as_f64().unwrap_or(1.),
                        v["width"].as_f64().unwrap_or(0.),
                        v["height"].as_f64().unwrap_or(0.),
                    )?;
                    b.refresh()?;
                    self.model.update(b.tasks.clone());
                    refresh = b.drain()?;
                }
                "region" => {
                    b.dimensions(
                        v["scale"].as_f64().unwrap_or(1.),
                        v["width"].as_f64().unwrap_or(0.),
                        v["height"].as_f64().unwrap_or(0.),
                    );
                    b.shape(serde_json::from_value(v["rects"].clone())?)?;
                }
                "area" => {
                    refresh = b.set_area(serde_json::from_value::<Rect>(v["rect"].clone())?);
                }
                "hidden" => b.hidden = v["hidden"].as_bool().unwrap_or(false),
                "events" => {
                    refresh = b.drain()?;
                }
                "refresh" => {
                    b.refresh()?;
                    self.model.update(b.tasks.clone());
                    refresh = b.drain()?;
                }
                "fallback" => {
                    b.query_pointer()?;
                    if b.hidden && b.edge {
                        b.recovering = true;
                    }
                    refresh = b.drain()?;
                }
                "timeout" => self.model.rebuild(),
                "activateWindow" => b.action(crate::model::Action::Activate(wid))?,
                "minimizeWindow" => b.action(crate::model::Action::Minimize(wid))?,
                "closeWindow" => b.action(crate::model::Action::Close(wid))?,
                "activate" => {
                    for a in self.model.activate(key)? {
                        b.action(a)?;
                    }
                }
                "perform" => {
                    for a in self
                        .model
                        .perform(v["action"].as_str().unwrap_or(""), key, wid)?
                    {
                        b.action(a)?;
                    }
                }
                "pinFiles" => {
                    self.model.pin_files(
                        &serde_json::from_value::<Vec<String>>(v["urls"].clone())?,
                        v["position"].as_i64().unwrap_or(-1) as i32,
                    )?;
                    result = json!(true);
                }
                "acceptsFiles" => {
                    let urls = serde_json::from_value::<Vec<String>>(v["urls"].clone())?;
                    result = json!(Model::local_files(&urls).is_ok());
                }
                "reorder" => {
                    result = json!(self.model.reorder(key, v["target"].as_str().unwrap_or("")))
                }
                "availableApplications" => {
                    self.model.reload_catalog();
                    result = self.model.available_applications();
                }
                "pinApplication" => {
                    self.model.pin_application(v["id"].as_str().unwrap_or(""))?;
                    result = json!(true);
                }
                "catalogIcon" => result = self.model.catalog_icon(v["id"].as_str().unwrap_or("")),
                "menu" => result = self.model.menu(key),
                "previews" => result = self.model.previews(key),
                "preference" => self.model.config.preference(
                    v["name"].as_str().unwrap_or(""),
                    v["value"].as_i64().unwrap_or(0) as i32,
                ),
                "resetPreferences" => self.model.config.reset_preferences(),
                "autoHide" => self.model.config.auto_hide = v["enabled"].as_bool().unwrap_or(true),
                _ => (),
            }
            Ok(())
        })();
        run.map_err(|e| e.to_string())?;
        refresh |= self.backend.drain().map_err(|e| e.to_string())?;
        Ok((result, refresh))
    }
    fn response(&mut self, result: Value, refresh: bool, error: Option<String>) -> Value {
        let mut out = json!({"result":result,"refreshNeeded":refresh,"fd":self.backend.fd(),"fallback":self.backend.window!=0&&(self.backend.hidden||self.backend.recovering),"pointerInside":self.backend.inside,"edgeHovered":self.backend.edge,"desktopVisible":self.backend.desktop_visible,"dockObstructed":self.backend.obstructed,"launchTimeout":self.model.next_timeout()});
        if self.last_groups.as_ref() != Some(&self.model.groups) {
            out["groups"] = json!(self.model.groups);
            self.last_groups = Some(self.model.groups.clone());
        }
        if self.last_tasks.as_ref() != Some(&self.backend.tasks) {
            out["tasks"] = json!(self.backend.tasks);
            self.last_tasks = Some(self.backend.tasks.clone());
        }
        if self.last_config.as_ref() != Some(&self.model.config) {
            out["config"] = json!(self.model.config);
            self.last_config = Some(self.model.config.clone());
        }
        if !self.backend.images.is_empty() {
            out["images"] = json!(std::mem::take(&mut self.backend.images));
        }
        if let Some(e) = error {
            out["error"] = json!(e);
        }
        out
    }
}
pub fn dispatch(slot: &mut Option<Engine>, request: &str) -> String {
    let run = (|| -> Result<Value, String> {
        let v: Value = serde_json::from_str(request).map_err(|e| e.to_string())?;
        if v["op"] == "init" {
            *slot = Some(Engine::new(&v)?);
        }
        let engine = slot
            .as_mut()
            .ok_or("Rust engine has not been initialized")?;
        let (result, refresh, error) = match engine.command(&v) {
            Ok((r, f)) => (r, f, None),
            Err(e) => (json!(false), false, Some(e)),
        };
        let fatal = error.is_some()
            && matches!(
                v["op"].as_str(),
                Some("attach" | "events" | "refresh" | "fallback" | "region")
            );
        let mut response = engine.response(result, refresh, error);
        if fatal {
            response["fatal"] = json!(true);
        }
        Ok(response)
    })();
    match run {
        Ok(v) => v.to_string(),
        Err(e) => json!({"error":e,"fatal":true}).to_string(),
    }
}
