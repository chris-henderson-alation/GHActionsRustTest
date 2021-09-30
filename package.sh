#!/usr/bin/env bash

set -e

export REGISTRY="${REGISTRY:-registry.kube-system}"

for service in service/* ; do
  (
    cd "$service" || exit 1
    ./package.sh
  )
done
