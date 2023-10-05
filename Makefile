TAG := bdk-reserves-web

run: builder
	docker run --rm --tty --env PORT=8888 --publish 8888:8888 ${TAG}

builder:
	docker build --tag ${TAG} .
