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

rm -rf "${TARGET_IMAGES:?}"/"${SERVICE_NAME}"
mkdir -p "${TARGET_IMAGES}"/"${SERVICE_NAME}"
cp Dockerfile "${TARGET_IMAGES}"/"${SERVICE_NAME}"
cp "${TARGET}"/release/"${SERVICE_NAME}" "${TARGET_IMAGES}"/"${SERVICE_NAME}"/
cd "${TARGET_IMAGES}"/"${SERVICE_NAME}"
REFERENCE="${REGISTRY}"/ocf-system/"${SERVICE_NAME}":"${VERSION}"
docker build -t "${REFERENCE}" .
docker save -o "${SERVICE_NAME}" "${REFERENCE}"
docker rmi "${REFERENCE}"
