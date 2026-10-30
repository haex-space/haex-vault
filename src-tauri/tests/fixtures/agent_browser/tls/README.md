# Test certificate of the agent browser tests

A throwaway self-signed certificate for `127.0.0.1` and `localhost` (valid for 100 years), so the
fixture site can also run over HTTPS: an agent may operate only websites over `https` (spec 065,
research R9). It protects nothing; the tests tell their Chromium to accept it.

Made with:

```sh
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -keyout key.pem \
  -out cert.pem -days 36500 -subj "/CN=holzi agent browser test" \
  -addext "subjectAltName=IP:127.0.0.1,DNS:localhost"
```
