use gtk::prelude::*;
use gtk::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use gtk::subclass::prelude::ObjectSubclass;
use gtk::{gio, glib, Application, ApplicationWindow, Button, ScrolledWindow, GridView, SignalListItemFactory, SingleSelection, PolicyType};
use glib::object::Object;
use crate::glib::{MainContext, PRIORITY_DEFAULT, clone, ParamSpecInt64, ParamFlags, ParamSpec, Value, ParamSpecString};
use serde::{Serialize, Deserialize};
use serde_json::json;
use std::time::Duration;
use std::thread;
use std::env;
use reqwest::StatusCode;

#[derive(Default)]
pub struct SlotObjectData {
  slot: Cell<i64>,
  machine: RefCell<String>,
  name: RefCell<String>,
  cost: Cell<i64>,
}

impl ObjectImpl for SlotObjectData {
  fn properties() -> &'static [ParamSpec] {
    use once_cell::sync::Lazy;

    static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
      vec![ParamSpecInt64::new(
        "slot",
        "slot",
        "slot",
        i64::MIN,
        i64::MAX,
        0,
        ParamFlags::READWRITE,
      ), ParamSpecInt64::new(
        "cost",
        "cost",
        "cost",
        i64::MIN,
        i64::MAX,
        0,
        ParamFlags::READWRITE,
      ), ParamSpecString::new(
        "name",
        "name",
        "name",
        None,
        ParamFlags::READWRITE,
      ), ParamSpecString::new(
        "machine",
        "machine",
        "machine",
        None,
        ParamFlags::READWRITE,
      )]
    });

    PROPERTIES.as_ref()
  }

  fn property(&self, _obj: &Self::Type, _id: usize, pspec: &ParamSpec) -> Value {
    match pspec.name() {
      "cost" => self.cost.get().to_value(),
      "slot" => self.slot.get().to_value(),
      "name" => self.name.borrow().to_value(),
      "machine" => self.machine.borrow().to_value(),
      _ => unimplemented!(),
    }
  }

  fn set_property(&self, _obj: &Self::Type, _id: usize, value: &Value, pspec: &ParamSpec) {
    match pspec.name() {
      "cost" => {
        self.cost.replace(value.get().unwrap());
      },
      "slot" => {
        self.slot.replace(value.get().unwrap());
      },
      "name" => {
        self.name.replace(value.get().unwrap());
      },
      "machine" => {
        self.machine.replace(value.get().unwrap());
      },
      _ => unimplemented!(),
    };
  }
}

#[glib::object_subclass]
impl ObjectSubclass for SlotObjectData {
  const NAME: &'static str = "SlotObject";
  type Type = SlotObject;
}

glib::wrapper! {
  pub struct SlotObject(ObjectSubclass<SlotObjectData>);
}

impl SlotObject {
  pub fn from_slot(machine: &Machine, slot: &Slot) -> Self {
    Object::new(&[
      ("slot", &slot.number),
      ("machine", &machine.name.to_string()),
      ("name", &slot.item.name),
      ("cost", &slot.item.price)
    ]).expect("Failed to create `SlotObject`.")
  }
}

fn main() {
  // Create a new application
  let app = Application::builder()
    .application_id("edu.rit.csh.mineral")
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
pub struct Machine {
  display_name: String,
  id: i64,
  is_online: bool,
  name: String,
  slots: Vec<Slot>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct Slot {
  active: bool,
  count: Option<i64>,
  empty: bool,
  item: Item,
  machine: i64,
  number: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
pub struct Item {
  id: i64,
  name: String,
  price: i64,
}

fn build_ui(app: &Application) {
  let displayable_machines: Vec<i64> = env::var("DISPLAYABLE_MACHINES")
    .unwrap().to_string().split(",")
    .map(|id| id.parse::<i64>().unwrap())
    .collect();
  let endpoint = "https://drink.csh.rit.edu";
  let drink_model = gio::ListStore::new(SlotObject::static_type());
  let (drinks_tx, drinks_rx) = MainContext::channel(PRIORITY_DEFAULT);

  thread::spawn(move || {
    let secret = env::var("MACHINE_SECRET").unwrap().to_string();
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
        drinks_tx.send(res).unwrap();
      }

      let one_minute = Duration::from_secs(60);
      thread::sleep(one_minute);
    }
  });

  drinks_rx.attach(
    None,
    clone!(
      @weak drink_model => @default-return Continue(false),
      move |res| {
        let slot_objects = res.machines.iter()
          .filter(|machine| displayable_machines.contains(&machine.id) &&
                  machine.is_online)
          .flat_map(|machine| machine.slots
                    .iter()
                    .filter(|slot| slot.active &&
                            !slot.empty &&
                            slot.count.map_or(true, |count| count > 0))
                    .map(|slot| SlotObject::from_slot(machine, slot)))
          .collect::<Vec<SlotObject>>();

        drink_model.splice(0, drink_model.n_items(), &slot_objects);
        Continue(true)
      }
    )
  );

  let factory = SignalListItemFactory::new();
  factory.connect_setup(move |_, list_item| {
    let button = Button::builder()
      .build();
    list_item.set_child(Some(&button));
  });
  factory.connect_bind(move |_, list_item| {
    let slot_object = list_item
      .item()
      .expect("Slot must exist!")
      .downcast::<SlotObject>()
      .expect("Slot must be a `SlotObject`!");

    let machine_id = slot_object.property::<String>("machine");
    let slot_id = slot_object.property::<i64>("slot");
    let item_name = slot_object.property::<String>("name");
    let item_cost = slot_object.property::<i64>("cost");

    let button = list_item
      .child()
      .expect("The child has to exist.")
      .downcast::<Button>()
      .expect("The child must be a `Button`!");
    
    button.set_label(&format!("{} - {}", item_name, item_cost));

    button.connect_clicked(move |_button| {
      println!("clicked!");
      let secret = env::var("MACHINE_SECRET").unwrap().to_string();
      let http = reqwest::blocking::Client::new();
      let res = http.post(endpoint.clone().to_owned() + "/drinks/drop")
        .header("X-Auth-Token", secret.clone())
        .header("X-User-Info", &json!({
          "preferred_username": "mstrodl",
        }).to_string())
        .json(&json!({
          "machine": machine_id,
          "slot": slot_id,
        }))
        .send();
      match res {
        Ok(res) => println!("Got a {} response!", res.status()),
        Err(err) => println!(":( {:?}", err)
      }
    });
  });
  let selection_model = SingleSelection::new(Some(&drink_model));
  let drink_list = GridView::builder()
    .model(&selection_model)
    .factory(&factory)
    .max_columns(3)
    .build();

  let scrolled_window = ScrolledWindow::builder()
    .hscrollbar_policy(PolicyType::Never) // Disable horizontal scrolling
    .min_content_width(360)
    .child(&drink_list)
    .build();

  // Create a window and set the title
  let mut window_builder = ApplicationWindow::builder()
    .application(app)
    .title("My GTK App")
    .child(&scrolled_window)
    .maximized(true);
  if !env::var("DEVELOPMENT").ok().map_or(false, |value| value == "true") {
    window_builder = window_builder.fullscreened(true);
  }
  let window = window_builder.build();

  // Present window
  window.present();
}
