FROM gcr.io/distroless/static:latest
COPY containerd /usr/local/bin/containerd
COPY ./etc/containerd /etc/containerd
ENTRYPOINT ["/usr/local/bin/containerd"]