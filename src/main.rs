use gtk::prelude::*;
use gatekeeper_members::GateKeeperMemberListener;
use gtk::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use gtk::subclass::prelude::ObjectSubclass;
use gtk::{gio, glib, Application, ApplicationWindow, Button, ScrolledWindow, GridView, SignalListItemFactory, SingleSelection, PolicyType, CenterBox, Box, Orientation, Label};
use gio::ApplicationFlags;
use glib::object::Object;
use crate::glib::{MainContext, PRIORITY_DEFAULT, clone, ParamSpecInt64, ParamFlags, ParamSpec, Value, ParamSpecString};
use serde::{Serialize, Deserialize};
use serde_json::json;
use std::time::Duration;
use std::thread;
use std::env;
use reqwest::StatusCode;
use clap::Arg;
use libgatekeeper_sys::Nfc;
use std::sync::mpsc;

#[derive(Default)]
pub struct SlotObjectData {
  slot: Cell<i64>,
  machine: RefCell<String>,
  name: RefCell<String>,
  cost: Cell<i64>,
}

enum OrderingState {
  PleaseScan(mpsc::Sender<()>),
  Vending(String),
  Failed(String),
  Dropped(String),
  Finished(bool),
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
    .flags(ApplicationFlags::HANDLES_COMMAND_LINE)
    .build();
  let (cmd_tx, cmd_rx) = mpsc::channel();
  app.connect_command_line(move |app, cli| {
    println!("Got cli!");
    cmd_tx.send(cli.clone()).unwrap();
    app.activate();
    0
  });
  // Connect to "activate" signal of `app`
  app.connect_activate(move |app: &Application| {
    println!("Activating");
    let command = clap::Command::new("Mineral")
      .version("0.1.0")
      .author("Mary Strodl <ipadlover8322@gmail.com>")
      .about("Touch screen drink client")
      .arg(Arg::new("DEVICE")
           .help("Device connection string (e.g. 'pn532_uart:/dev/ttyUSB0')")
           .required(true)
           .index(1));

    let matches = command.get_matches_from(cmd_rx.recv().unwrap().arguments());
    let conn_str = matches.value_of("DEVICE").unwrap().to_string(); 
    
    println!("Got conn!");
    build_ui(app, conn_str);
  });
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

fn build_ui(app: &Application, conn_str: std::string::String) {
  let displayable_machines: Vec<i64> = env::var("DISPLAYABLE_MACHINES")
    .unwrap().to_string().split(",")
    .map(|id| id.parse::<i64>().unwrap())
    .collect();
  let endpoint = "https://drink.csh.rit.edu";
  let drink_model = gio::ListStore::new(SlotObject::static_type());
  let (drinks_tx, drinks_rx) = MainContext::channel(PRIORITY_DEFAULT);
  let (ordering_tx, ordering_rx) = MainContext::channel(PRIORITY_DEFAULT);
  let (poll_tx, poll_rx) = mpsc::channel();

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
      if let Ok(_) = poll_rx.recv_timeout(one_minute) {
        println!("Fetching new data because a drink was dropped!");
      }
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
    .title("Mineral")
    .child(&scrolled_window)
    .maximized(true);
  if !env::var("DEVELOPMENT").ok().map_or(false, |value| value == "true") {
    window_builder = window_builder.fullscreened(true);
  }
  let window = window_builder.build();

  factory.connect_setup(move |_, list_item| {
    let button = Button::builder()
      .build();
    list_item.set_child(Some(&button));
  });
  factory.connect_bind(move |_, list_item| {
    let ordering_tx = ordering_tx.clone();
    let conn_str = conn_str.clone();
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
      let conn_str = conn_str.clone();
      let machine_id = machine_id.clone();
      let item_name = item_name.clone();
      let item_cost = item_cost.clone();
      let ordering_tx = ordering_tx.clone();
      thread::spawn(move || {
        let (cancel_tx, cancel_rx) = mpsc::channel();
        ordering_tx.send(OrderingState::PleaseScan(cancel_tx)).unwrap();
        println!("clicked!");

        let mut nfc = Nfc::new().unwrap();
        let mut member_listener = GateKeeperMemberListener::new(&mut nfc, conn_str.to_string()).unwrap();

        let uid = loop {
          let association = loop {
            if let Some(association) = member_listener.poll_for_user() {
              break association;
            }
            if let Ok(_) = cancel_rx.recv_timeout(Duration::from_millis(250)) {
              ordering_tx.send(OrderingState::Finished(false)).unwrap();
              return;
            }
          };

          let user = match member_listener.fetch_user(association.clone()) {
            Ok(user) => user,
            Err(_) => {
              eprintln!("Couldn't fetch user for association {}!", association);
              continue
            }
          };

          break user["user"]["uid"].as_str().unwrap().to_string()
        };
        ordering_tx.send(OrderingState::Vending(
          format!("Dropping {}...", item_name))).unwrap();

        let secret = env::var("MACHINE_SECRET").unwrap().to_string();
        let http = reqwest::blocking::Client::new();
        println!("Dropping a drink!");
        let res = http.post(endpoint.clone().to_owned() + "/drinks/drop")
          .header("X-Auth-Token", secret.clone())
          .header("X-User-Info", &json!({
            "preferred_username": uid,
          }).to_string())
          .json(&json!({
            "machine": machine_id.clone(),
            "slot": slot_id,
          }))
          .send();
        println!("Looks like we got a response");
        match res {
          Ok(res) => match res.status() {
            StatusCode::OK => ordering_tx.send(OrderingState::Dropped(
              format!("Dropped {} for {} credits. Enjoy!", item_name, item_cost))).unwrap(),
            code =>
              ordering_tx.send(OrderingState::Failed(format!(
                "Error: Got a {} response from the server. Try again later", code)
              )).unwrap(),
          },
          Err(err) => {
            eprintln!("Failed to drop slot {} from {}: {:?}",
                      slot_id, machine_id.clone(), err);
            ordering_tx.send(OrderingState::Failed(
              format!("Failed to drop: {:?}", err))).unwrap()
          }
        };
        println!("I think we dropped! Waiting a bit to let the user read");

        // Allow the message to show for a bit
        thread::sleep(Duration::from_secs(5));

        println!("Bailing back to menu after drop");
        ordering_tx.send(OrderingState::Finished(true)).unwrap();
      });
    });
  });

  let info_box = CenterBox::builder()
    .hexpand(true)
    .build();

  ordering_rx.attach(
    None,
    clone!(
      @weak window => @default-return Continue(false),
      move |state| {
        match state {
          OrderingState::PleaseScan(cancel_tx) => {
            let please_scan = Box::builder()
              .orientation(Orientation::Vertical)
              .build();
            please_scan.append(&Label::builder()
                               .label("Please scan your tag!")
                               .build());
            let cancel_button = Button::builder()
              .label("Cancel")
              .build();
            please_scan.append(&cancel_button);
            cancel_button.connect_clicked(move |_button| {
              cancel_tx.send(()).unwrap();
            });
            info_box.set_center_widget(Some(&please_scan));
            window.set_child(Some(&info_box));
            ()
          },
          OrderingState::Vending(content) |
          OrderingState::Failed(content) |
          OrderingState::Dropped(content) => {
            info_box.set_center_widget(
              Some(&Label::builder()
                   .label(&content)
                   .build())
            );
          },
          OrderingState::Finished(poll) => {
            window.set_child(Some(&scrolled_window));
            if poll {
              poll_tx.send(()).unwrap();
            }
          },
        }
        Continue(true)
      }
    )
  );

  // Present window
  window.present();
}