#!/usr/bin/env bash

TAG=just-a-test
REFERENCE=248135293344.dkr.ecr.us-east-2.amazonaws.com/ocf-system/acm:"${TAG}"

cat > Dockerfile <<EOF
FROM alpine:latest
ENTRYPOINT ["/bin/echo", "Hello world"]
EOF

docker login --username AWS --password-stdin "$(aws ecr get-login-password --region us-east-2)" 248135293344.dkr.ecr.us-east-2.amazonaws.com

docker build -t "${REFERENCE}" .
docker push "${REFERENCE}"
