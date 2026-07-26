FROM node:22.14.0-bookworm-slim AS build

WORKDIR /workspace
COPY frontend/web/package.json frontend/web/package-lock.json ./
RUN npm ci
COPY frontend/web ./
RUN npm run build

FROM nginx:1.27.4-alpine
COPY frontend/web/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=build /workspace/dist /usr/share/nginx/html
EXPOSE 80
