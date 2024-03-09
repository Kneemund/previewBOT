# previewBOT

A Discord bot written in Rust for sending file previews of GitHub URLs and for juxtaposing images.

## Invite

You can invite the official instance of this bot to your server.

> https://previewbot.kneemund.de/invite

## Details

Provides an HTTP API for detailed views of juxtaposed images on the web. The source code of the website used for the official instance of this bot is available [here](https://github.com/Kneemund/juxtapose).

A container image can be built by using the provided `Dockerfile`. It supports fast multi-architecture builds for amd64, aarch64 and arm/v7 using cross compilation instead of emulation. The produced binaries are fully statically-linked using `musl` and `mold`. As such, the image is derived from the empty `scratch` base image and only contains the binary.

Running the bot with the `--reload-commands` argument will register all slash commands after connecting to the Discord API. This is only necessary on new accounts or after changes to the structure of slash commands.

## Enviornment Variables

All environment variables without a default value must be specified, otherwise the application will panic (usually during startup). If a `.env` file exists within the working directory, the location of the file is logged and it will be parsed and loaded while keeping the values of already existing environment variables.

| Name                | Default Value        | Description                                                                                                                                                            |
| ------------------- | -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| BOT_TOKEN           | NONE                 | Secret token for the bot account created in the Discord Developer Portal.                                                                                              |
| BLAKE3_KEY_MATERIAL | NONE                 | Master secret key for deriving other keys using the BLAKE3 KDF, e.g. the key for creating and validating the HMAC in Juxtapose URLs.                                   |
| JUXTAPOSE_BASE_URL  | `http://localhost`   | Base URL used for viewing juxtaposed images, used for generating URLs for the "Open" button.                                                                           |
| REDIS_URL           | `redis://127.0.0.1/` | URL used for connecting to Redis. Can be either a TCP connection (`redis://` or `rediss://`), or an IPC/UNIX connection (`redis+unix://`).                             |
| PORT                | NONE                 | Port number that the HTTP API runs on.                                                                                                                                 |
| SOCKET_PATH         | NONE                 | UNIX Domain Socket path that the HTTP API runs on. Only supported on UNIX systems, takes precendence over PORT.                                                        |
| CORS_ORIGIN         | `*`                  | Allowed origin domains for CORS. Allows all domains by default, but is highly recommended to be set to a specific domain in production (typically JUXTAPOSE_BASE_URL). |

## Running Binaries using Podman & Quadlets

You can run the provided [image](https://github.com/Kneemund/previewBOT/pkgs/container/previewbot) using rootless Podman. This section describes how you can use the systemd integration, Quadlets, to setup user-space systemd services for Redis and previewBOT with persistent storage and communication via Unix Domain Sockets.

After creating the `.container` files at the bottom of this section, run `systemctl --user daemon-reload` to generate the systemd service files. The configuration makes a few assumptions about the user's home directory.

-   All of the folders and files mentioned below are owned and writable by the current user.
-   `~/production/previewbot/previewbot.env` is an existing file that contains the environment variables described in the section above. Among other things, it must contain `REDIS_URL=redis+unix:///run/redis/redis.sock`.
-   `~/production/redis/data` is an existing folder used to store RDB dumps for persistent storage.
-   `~/production/redis/conf/redis.conf` is an existing file with the following content for configuring the Unix Domain Socket.

```
unixsocket /run/redis/redis.sock
unixsocketperm 777
port 0
```

Then, run previewBOT using `systemctl --user start previewbot.service`. previewBOT will be started automatically after rebooting.

---

`~/.config/containers/systemd/previewbot.container`

```service
[Unit]
Description=previewBOT container
After=network-online.target

[Container]
Image=ghcr.io/kneemund/previewbot:latest
Volume=redis-unix-socket:/run/redis
EnvironmentFile=%h/production/previewbot/previewbot.env
Network=host
AutoUpdate=registry

[Install]
WantedBy=default.target
```

---

`~/.config/containers/systemd/redis.container`

```service
[Unit]
Description=Redis container
After=network-online.target

[Container]
Image=docker.io/redis:alpine
Volume=%h/production/redis/data:/data
Volume=%h/production/redis/conf:/usr/local/etc/redis
Volume=redis-unix-socket:/run/redis
UserNS=keep-id:uid=999,gid=1000
Network=none
AutoUpdate=registry
Exec=redis-server /usr/local/etc/redis/redis.conf

[Install]
WantedBy=previewbot.service
```
