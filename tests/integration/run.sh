#!/bin/sh
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
compose_file="$project_dir/tests/integration/docker-compose.yml"
cert_dir="$project_dir/tests/integration/.certs"

mkdir -p "$cert_dir"
if [ ! -f "$cert_dir/ca.crt" ]; then
  openssl req -x509 -nodes -newkey rsa:2048 -sha256 -days 2 \
    -subj '/CN=Harbor Transfer Integration CA' \
    -addext 'basicConstraints=critical,CA:TRUE' \
    -addext 'keyUsage=critical,keyCertSign,cRLSign' \
    -keyout "$cert_dir/ca.key" -out "$cert_dir/ca.crt"
  openssl req -nodes -newkey rsa:2048 -sha256 \
    -subj '/CN=localhost' \
    -addext 'subjectAltName=DNS:localhost,IP:127.0.0.1' \
    -addext 'extendedKeyUsage=serverAuth' \
    -addext 'keyUsage=critical,digitalSignature,keyEncipherment' \
    -keyout "$cert_dir/server.key" -out "$cert_dir/server.csr"
  openssl x509 -req -sha256 -days 2 \
    -in "$cert_dir/server.csr" \
    -CA "$cert_dir/ca.crt" -CAkey "$cert_dir/ca.key" -CAcreateserial \
    -copy_extensions copy -out "$cert_dir/server.crt"
fi

cleanup() {
  status=$?
  if [ "$status" -ne 0 ]; then
    echo "Protocol integration failed; collecting service logs."
    docker compose -f "$compose_file" logs --no-color || true
  fi
  docker compose -f "$compose_file" down --volumes --remove-orphans
}
trap cleanup EXIT INT TERM

docker compose -f "$compose_file" up --build --wait

FTP_TEST_HOST=127.0.0.1 FTP_TEST_PORT=2121 FTP_TEST_USER=harbor FTP_TEST_PASS=harbor \
  cargo test --manifest-path "$project_dir/src-tauri/Cargo.toml" ftp_client::tests::test_ftp_ -- --test-threads=1

FTP_TEST_CA_CERT="$cert_dir/ca.crt" FTP_TEST_HOST=127.0.0.1 FTP_TEST_PORT=2990 FTP_TEST_USER=harbor FTP_TEST_PASS=harbor FTP_TEST_TLS=1 \
  cargo test --manifest-path "$project_dir/src-tauri/Cargo.toml" ftp_client::tests::test_ftp_ -- --test-threads=1

SFTP_TEST_HOST=127.0.0.1 SFTP_TEST_PORT=2222 SFTP_TEST_USER=harbor SFTP_TEST_PASS=harbor \
  cargo test --manifest-path "$project_dir/src-tauri/Cargo.toml" sftp_client::tests::test_sftp_live_ -- --test-threads=1

WEBDAV_TEST_CA_CERT="$cert_dir/ca.crt" WEBDAV_TEST_HOST=127.0.0.1 WEBDAV_TEST_PORT=8443 WEBDAV_TEST_USER=harbor WEBDAV_TEST_PASS=harbor \
  cargo test --manifest-path "$project_dir/src-tauri/Cargo.toml" webdav_client::tests::live_webdav_ -- --test-threads=1

WEBDAV_TEST_CA_CERT="$cert_dir/ca.crt" WEBDAV_TEST_HOST=127.0.0.1 WEBDAV_TEST_PORT=8444 WEBDAV_TEST_USER=harbor WEBDAV_TEST_PASS=harbor WEBDAV_TEST_ROOT=/remote.php/dav/files/harbor \
  cargo test --manifest-path "$project_dir/src-tauri/Cargo.toml" webdav_client::tests::live_webdav_ -- --test-threads=1
