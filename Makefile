TAG := bdk-reserves-web
ALPINE_REPO := http://dl-cdn.alpinelinux.org/alpine/v3.18
MITM_CA := ""

run: builder
	docker run --rm --tty --env PORT=8888 --publish 8888:8888 ${TAG}

builder:
	docker build --tag ${TAG} --build-arg "ALPINE_REPO=${ALPINE_REPO}" --build-arg "MITM_CA=${MITM_CA}" .
