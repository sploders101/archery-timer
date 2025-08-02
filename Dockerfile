FROM docker.io/sploders101/devcontainer:latest
USER root:root
RUN apt-get update && apt-get install -y \
  pkg-config \
  libdbus-1-dev \
  libasound-dev \
  libgtk-3-dev \
  && rm -rf /var/lib/apt/lists/*
USER dev:dev
