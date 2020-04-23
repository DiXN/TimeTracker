use std::error::Error;
use std::thread;

use reqwest::Client;

use serde_json::{json, Value};

use chrono::prelude::*;

use crossbeam_channel::Receiver;

use crate::restable::Restable;
use crate::receive_types::ReceiveTypes;

#[derive(Clone)]
pub struct FirebaseClient {
  pub authentication: String,
  pub base_url: String,
}

impl FirebaseClient {
  pub fn new(base_url: &str, authentication: &str) -> FirebaseClient {
    FirebaseClient {
      authentication: authentication.to_owned(),
      base_url: base_url.to_owned()
    }
  }

  fn patch_data(&self, item: &str, value: &Value) -> Result<Value, Box<dyn Error>> {
    let url = format!("{}/apps/{}.json?auth={}", &self.base_url, item, &self.authentication);

    Ok(Client::new()
      .patch(&url)
      .json(value)
      .send()
      .and_then(|mut res| Ok(res.json::<Value>()?)
    )?)
  }
}

impl Restable for FirebaseClient {
  fn setup(&self) -> Result<(), Box<dyn Error>> {
    Ok(())
  }

  fn get_data(&self, item: &str) -> Result<Value, Box<dyn Error>> {
    let url = format!("{}/{}.json?auth={}", &self.base_url, item, &self.authentication);
    Ok(serde_json::from_str(&reqwest::get(&url)?.text()?)?)
  }

  fn get_processes(&self) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(self.get_data("/apps/")?
        .as_object()
        .unwrap()
        .into_iter()
        .map(|(key, _)| key.to_owned())
        .collect::<Vec<String>>())
  }

  fn put_data(&self, item: &str, product_name: &str) -> Result<Value, Box<dyn Error>> {
    let url = format!("{}/apps/{}.json?auth={}", &self.base_url, item, &self.authentication);

    Ok(Client::new()
      .put(&url)
      .json(&json!({
        "duration": 0,
        "launches": 0,
        "longestSession": 0,
        "name": item,
        "productName": if product_name.is_empty() {
          item
        } else {
          product_name
        }
      }))
      .send()
      .and_then(|mut res| Ok(res.json::<Value>()?)
    )?)
  }

  fn delete_data(&self, item: &str) -> Result<Value, Box<dyn Error>> {
    let url = format!("{}/apps/{}.json?auth={}", &self.base_url, item, &self.authentication);

    Ok(Client::new()
      .delete(&url)
      .send()
      .and_then(|mut res| Ok(res.json::<Value>()?)
    )?)
  }

  fn init_event_loop(self, rx: Receiver<(String, ReceiveTypes)>) {
    thread::spawn(move || {
      let patch_increment = |item: &str, inc_type: &str| {
        if let Ok(ret) = self.get_data(&format!("/apps/{}/{}", item, inc_type)) {
          let mut inc = ret.as_i64().unwrap();
          inc += 1i64;

          if let Ok(o) = self.patch_data(&item, &json!({inc_type: inc})) {
            info!("{}: {}", item, o);
          }
        }
      };

      while let Ok(rx) = rx.recv() {
        match rx {
          (item, ReceiveTypes::LONGEST_SESSION) => {
            let split = item.split(";").collect::<Vec<&str>>();
            if let Ok(ret) = self.get_data(&format!("/apps/{}/longestSession", split[0])) {
              let longest_session = ret.as_i64().unwrap();
              let current_session = split[1].parse::<i64>().unwrap();

              if current_session > longest_session {
                if let Ok(o) = self.patch_data(split[0], &json!({"longestSession": current_session})) {
                  info!("{}: {}", split[0], o);
                }
              }
            }
          },
          (item, ReceiveTypes::DURATION) => patch_increment(&item, "duration"),
          (item, ReceiveTypes::LAUNCHES) => patch_increment(&item, "launches"),
          (item, ReceiveTypes::TIMELINE) => {
            let dt = Local::now();
            let date_str = format!("apps/{}/timeline/year/{}/{}/{}", item, dt.year().to_string(), (dt.month() - 1).to_string(), dt.day().to_string());

            if let Ok(ret) = self.get_data(&date_str) {
              let current_value = if let Some(mut inc) = ret.as_i64() {
                inc += 1i64;
                inc
              } else {
                1
              };

              let date_str = format!("{}/timeline/year/{}/{}", item, dt.year().to_string(), (dt.month() - 1).to_string());
              if let Ok(o) = self.patch_data(&date_str, &json!({dt.day().to_string(): current_value})) {
                info!("{}: {}", item, o);
              }
            };
          }
        };
      }
    });
  }
}
