# previewBOT

A Discord bot written in Rust for sending file previews of GitHub URLs and for juxtaposing images.

## Invite

You can invite the official instance of this bot to your server.

> https://previewbot.kneemund.de/invite

## Details

Provides an HTTP API for detailed views of juxtaposed images on the web. The source code of the website used for the official instance of this bot is available [here](https://github.com/Kneemund/juxtapose).

A container image can be built by using the provided `Dockerfile`. It supports fast multi-architecture builds for amd64, aarch64 and arm/v7 using cross compilation instead of emulation. The produced binaries are fully statically-linked using `musl` and `mold`. As such, the image is derived from the empty `scratch` base image and only contains the binary.

Running the bot with the `--reload-commands` argument will register all slash commands after connecting to the Discord API. This is only necessary on new accounts or after changes to the structure of slash commands.

## Environment Variables

All environment variables without a default value must be specified, otherwise the application will panic (usually during startup). If a `.env` file exists within the working directory, the location of the file is logged, and it will be parsed and loaded while keeping the values of already existing environment variables.

| Name                | Default Value        | Description                                                                                                                                                            |
| ------------------- | -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| BOT_TOKEN           | NONE                 | Secret token for the bot account created in the Discord Developer Portal.                                                                                              |
| BLAKE3_KEY_MATERIAL | NONE                 | Master secret key for deriving other keys using the BLAKE3 KDF, e.g. the key for creating and validating the HMAC in Juxtapose URLs.                                   |
| JUXTAPOSE_BASE_URL  | `http://localhost`   | Base URL used for viewing juxtaposed images, used for generating URLs for the "Open" button.                                                                           |
| REDIS_URL           | `redis://127.0.0.1/` | URL used for connecting to Redis/Valkey. Can be either a TCP connection (`redis://` or `rediss://`), or an IPC/UNIX connection (`redis+unix://`).                      |
| PORT                | NONE                 | Port number that the HTTP API runs on.                                                                                                                                 |
| SOCKET_PATH         | NONE                 | UNIX Domain Socket path that the HTTP API runs on. Only supported on UNIX systems, takes precedence over PORT.                                                         |
| CORS_ORIGIN         | `*`                  | Allowed origin domains for CORS. Allows all domains by default, but is highly recommended being set to a specific domain in production (typically JUXTAPOSE_BASE_URL). |

## Running Binaries using Podman & Quadlets

You can run the provided [image](https://github.com/Kneemund/previewBOT/pkgs/container/previewbot) using rootless Podman. This section describes how you can use the systemd integration, Quadlets, to set up user-space systemd services for Valkey and previewBOT with persistent storage and communication via Unix Domain Sockets. The HTTP API is exposed using a local NGINX installation.

After creating the `.container` files at the bottom of this section, run `systemctl --user daemon-reload` to generate the systemd service files. The configuration makes a few assumptions about the user's home directory.

-   All the folders and files mentioned below are owned and writable by the current user.
-   `~/production/previewbot/previewbot.env` is an existing file that contains the environment variables described in the section above. Among other things, it must contain `REDIS_URL=redis+unix:///run/valkey/valkey.sock` and `SOCKET_PATH=/run/previewbot/api.sock`.
-   `~/production/valkey/data` is an existing folder used to store RDB dumps for persistent storage.
-   `~/production/valkey/conf/valkey.conf` is an existing file with the following content for configuring the Unix Domain Socket.

```
unixsocket /run/valkey/valkey.sock
unixsocketperm 777
port 0
```

The previewBOT Quadlet below maps the Unix Domain Socket containing the HTTP API to `/opt/previewbot/api.sock`. It can be exposed using the following NGINX configuration.

```nginx
upstream previewbot-api {
        server unix:/opt/previewbot/api.sock;
}

server {
        listen 443 ssl;
        listen [::]:443 ssl;

        ssl_certificate ...;
        ssl_certificate_key ...;

        server_tokens off;

        server_name ...;

        location /juxtapose/url {
                proxy_pass http://previewbot-api/url;
        }

	    location / {
                return 404;
        }
}
```

> When using SELinux, you'll most likely need to configure contexts and policies to allow NGINX to connect to the Unix Domain Socket.

The accompanying website to display the juxtaposed images can be found in [this repository](https://github.com/Kneemund/juxtapose). Note that the domain of the API URL is hard-coded in the JavaScript file and needs to be adapted.

Finally, run previewBOT using `systemctl --user start previewbot.service`. previewBOT will be started automatically after rebooting.

---

`~/.config/containers/systemd/previewbot.container`

```ini
[Unit]
Description=previewBOT container
After=network-online.target valkey.service
Requires=valkey.service
StartLimitInterval=200
StartLimitBurst=5

[Container]
Image=ghcr.io/kneemund/previewbot:latest
Volume=valkey-unix-socket:/run/valkey:Z
Volume=/opt/previewbot:/run/previewbot:Z
EnvironmentFile=%h/production/previewbot/previewbot.env
Network=host
AutoUpdate=registry

[Service]
Restart=always
RestartSec=30

[Install]
WantedBy=default.target
```

---

`~/.config/containers/systemd/valkey.container`

```ini
[Unit]
Description=Valkey container
After=network-online.target
StartLimitInterval=200
StartLimitBurst=5

[Container]
Image=docker.io/valkey/valkey:alpine
Volume=%h/production/valkey/data:/data:Z
Volume=%h/production/valkey/conf:/usr/local/etc/valkey:Z
Volume=valkey-unix-socket:/run/valkey:Z
UserNS=keep-id:uid=999,gid=1000
Network=none
AutoUpdate=registry
Exec=valkey-server /usr/local/etc/valkey/valkey.conf

[Service]
Restart=always
RestartSec=30

[Install]
WantedBy=previewbot.service
```
