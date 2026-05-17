# edgemail

A professional, production-grade SMTP server! No it's not.
This demo project implements a very simple temporary e-mail, capable of receiving e-mails and storing them in [libSQL](https://github.com/libsql/libsql) (locally), [sqld](https://github.com/libsql/sqld), or [Turso](https://chiselstrike.com).

Tutorial: [Write your own email server in Rust](https://blog.turso.tech/write-your-own-email-server-in-rust-36f4ff5b1956)

Example deployment: https://sorry.idont.date/

In order to get it to work, run it on a machine with public IP, port `25` exposed, and add all appropriate DNS entries - an `MX` entry and its corresponding `A` entry that points to the IP address where `edgemail` is deployed.

## inbox api

If you start `edgemail` with `--api-port <port>`, it also serves a JSON API on that port.

- `GET /inbox?inbox=<email@domain>` returns a list of messages with `date`, `recipients`, `sender`, `title`, and `id`
- `GET /inbox/<id>` returns a single message with `id`, `date`, `recipients`, `sender`, `title`, and `body`
- after 100 API requests have been served, later requests return `503 Service Unavailable`
- API requests time out after 30 seconds and return `504 Gateway Timeout`

## client

edgemail has a client you can run as a static webpage. Find all the files in client/ directory. The only thing that needs to be changed is the database URL and the `readonly_token` used to authenticate for read-only access.
