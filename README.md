# mineral

mineral is a next generation touch-screen implementation for
the [Computer Science House](https://csh.rit.edu/) drink machines!

Written in [Rust](https://rust-lang.org/) using [gtk4-rs](https://gtk-rs.org/)

## Name

> _Wouldn't be a drink project if the name weren't stupid, now would it?_

_Mineral_ is Irish slang for a soft drink.
Iron is a mineral, which oxidizes to produce Rust: the language this program is written in.

## Running

mineral uses
[gatekeeper-members](https://github.com/computersciencehouse/gatekeeper-members)
to read gatekeeper tags. This means we need a number of environment variables set:

```
GK_HTTP_ENDPOINT=https://gatekeeper.cs.house
# One of the secrets from $GK_DRINK_SECRETS
GK_SERVER_TOKEN=...

GK_REALM_DRINK_READ_KEY=...
GK_REALM_DRINK_AUTH_KEY=...
GK_REALM_DRINK_PUBLIC_KEY='-----BEGIN PUBLIC KEY-----
...
-----END PUBLIC KEY-----
'
```

In addition to these, mineral understands the following environment variables:

```
# The drink machine shared-secret, for talking to the drink API
MACHINE_SECRET=...
# Which machines should be displayed? Separated by commas
# Note: this refers to the `id` field on responses returned by the server
DISPLAYABLE_MACHINES=1,2,3
# Should we run in "development mode"? If 'true', don't fullscreen
DEVELOPMENT=true
```

Now, we can start mineral:

```
# replace /dev/ttyUSB0 with the gatekeeper reader serial port
mineral pn532_uart:/dev/ttyUSB0
```
