# eatmail

A professional, production-grade SMTP server! No it's not.
This demo project implements a very simple temporary e-mail, capable of receiving e-mails and storing them in [Turso](https://chiselstrike.com).

Example deployment: https://eatmail.p-sarna.workers.dev/ , hooked with the `idont.date` domain. Any e-mail sent to a recipient
at this domain will be visible on the webpage. 

In order to get it to work, run it on a machine with public IP, port `25` exposed, and add all appropriate DNS entries - an `MX` entry and its corresponding `A` entry that points to the IP address where `eatmail` is deployed.
