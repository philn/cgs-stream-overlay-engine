FROM docker.io/restreamio/gstreamer:latest-dev

WORKDIR /app

RUN apt update && apt install -y curl clang make gcc g++ libglib2.0-dev

RUN curl -sL https://deb.nodesource.com/setup_14.x | bash -
RUN apt install -y nodejs

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

ENV PATH "/root/.cargo/bin:/bin:/usr/bin/:/usr/local/bin:/sbin:/usr/sbin:/app/node_modules/.bin"

COPY package.json .
COPY . .

RUN npm install
RUN neon build --release

EXPOSE 3000

ENV LIBGL_ALWAYS_SOFTWARE "true"

#RUN adduser --disabled-password --gecos '' luser
#USER luser

ENTRYPOINT ["node", "."]