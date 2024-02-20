# cardbot

simple discord bot with a web server for managing a card list for arcade rhythm games

this program runs a discord bot and a http server on port 3000 exposing cards added by users on discord in a format compatible with a spicy program

in order to access the cards on the http server you need to make a request with a `password` header with the same value that was specified in the config

the docker container requires the config to be present at `/config.toml`, or you can use envars with the `CARDBOT` prefix
