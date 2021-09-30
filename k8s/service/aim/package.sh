#!/usr/bin/env bash

set -e

function abspath() {
  # $1 : relative filename
  echo "$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
}

ROOT=$(abspath ../../)
source "${ROOT}"/version.env
SERVICE_NAME="$(basename "$(pwd)")"
TARGET="${ROOT}"/target
TARGET_IMAGES="${TARGET}"/images
CACHE="${ROOT}"/.cache

stat "${CACHE}" > /dev/null 2>&1 || mkdir -p "${CACHE}"
stat "${CACHE}"/docker > /dev/null 2>&1 || (
  cd "${CACHE}"
  curl -o docker.tar.gz https://download.docker.com/linux/static/stable/x86_64/docker-20.10.8.tgz
  tar zxf docker.tar.gz
)

rm -rf "${TARGET_IMAGES:?}"/"${SERVICE_NAME}"
mkdir -p "${TARGET_IMAGES}"/"${SERVICE_NAME}"
cp Dockerfile "${TARGET_IMAGES}"/"${SERVICE_NAME}"
cp "${CACHE}"/docker/ctr "${TARGET_IMAGES}"/"${SERVICE_NAME}"/
cp "${TARGET}"/release/"${SERVICE_NAME}" "${TARGET_IMAGES}"/"${SERVICE_NAME}"/
cd "${TARGET_IMAGES}"/"${SERVICE_NAME}"
REFERENCE="${REGISTRY}"/ocf-system/"${SERVICE_NAME}":"${VERSION}"
docker build -t "${REFERENCE}" .
docker save -o "${SERVICE_NAME}" "${REFERENCE}"
docker rmi "${REFERENCE}"
