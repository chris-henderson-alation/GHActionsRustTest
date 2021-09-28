#!/usr/bin/env bash

#curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64-2.2.41.zip" -o "awscliv2.zip"
#unzip -q awscliv2.zip
#sudo ./aws/install
#alias aws=/usr/local/bin/aws


aws configure set AWS_ACCOUNT "${AWS_ACCOUNT}"
aws configure set AWS_ACCESS_KEY_ID "${AWS_ACCESS_KEY_ID}"
aws configure set AWS_SECRET_ACCESS_KEY "${AWS_SECRET_ACCESS_KEY}"
aws configure set AWS_REGION "${AWS_REGION}"
PASSWORD=$(aws ecr get-login-password --region east-us-2)

TAG=just-a-test
REFERENCE=248135293344.dkr.ecr.us-east-2.amazonaws.com/ocf-system/acm:"${TAG}"

cat > Dockerfile <<EOF
FROM alpine:latest
ENTRYPOINT ["/bin/echo", "Hello world"]
EOF


docker login --username AWS --password "${PASSWORD}" 248135293344.dkr.ecr.us-east-2.amazonaws.com

docker build -t "${REFERENCE}" .
docker push "${REFERENCE}"
