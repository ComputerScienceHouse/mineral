use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Button};
use serde::{Serialize, Deserialize};
use std::time::Duration;
use std::thread;
use std::env;
use reqwest::StatusCode;

fn main() {
  // Create a new application
  let app = Application::builder()
    .application_id("org.gtk-rs.example")
    .build();

  // Connect to "activate" signal of `app`
  app.connect_activate(build_ui);

  // Run the application
  app.run();
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
struct DrinksResponse {
  machines: Vec<Machine>,
  message: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
struct Machine {
  display_name: String,
  id: usize,
  is_online: bool,
  name: String,
  slots: Vec<Slot>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
struct Slot {
  active: bool,
  count: Option<usize>,
  empty: bool,
  item: Item,
  machine: usize,
  number: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
struct Item {
  id: usize,
  name: String,
  price: usize,
}

fn build_ui(app: &Application) {
  let http = reqwest::blocking::Client::new();
  let secret = env::var("MACHINE_SECRET").unwrap().to_string();
  let displayable_machines: Vec<usize> = env::var("DISPLAYABLE_MACHINES")
    .unwrap().to_string().split(",")
    .map(|id| id.parse::<usize>().unwrap())
    .collect();
  let endpoint = "https://drink.csh.rit.edu";
  thread::spawn(move || {
    let http = reqwest::blocking::Client::new();
    loop {
      println!("Trying to get soda");
      // Get new soda!
      let drinks = http.get(endpoint.clone().to_owned() + "/drinks")
        .header("X-Auth-Token", secret.clone())
        .send();
      println!("Got them drinks");
      let res = drinks.ok().and_then(|drinks| match drinks.status() {
        StatusCode::OK => drinks.json::<DrinksResponse>().ok(),
        _ => None,
      });
      if let Some(res) = res {
        let slots = res.machines.iter()
          .filter(|machine| displayable_machines.contains(&machine.id) &&
                  machine.is_online)
          .flat_map(|machine| &machine.slots)
          .filter(|slot| slot.active &&
                  !slot.empty &&
                  slot.count.map_or(true, |count| count > 0))
          .collect::<Vec<&Slot>>();
        for slot in slots {
          println!("{:?}", slot);
        }
      }

      let one_minute = Duration::from_secs(60);
      thread::sleep(one_minute);
    }
  });
  
  // Hello world button
  let button = Button::builder()
    .label("AAAAgh")
    .margin_top(12)
    .margin_bottom(12)
    .margin_start(12)
    .margin_end(12)
    .build();

  // Connect to "clicked" signal of `button`
  button.connect_clicked(move |button| {
    // Set the label to "Hello World!" after the button has been clicked on
    button.set_label("Hello World!");
  });
  
  // Create a window and set the title
  let window = ApplicationWindow::builder()
    .application(app)
    .title("My GTK App")
    .child(&button)
    .build();

  // Present window
  window.present();
}
