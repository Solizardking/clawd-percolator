FROM node:22-bookworm-slim
WORKDIR /srv
COPY deploy/hetzner/frontend/package.json deploy/hetzner/frontend/package-lock.json ./
RUN npm ci --omit=dev
COPY deploy/clawd-percolator.json /srv/deploy/clawd-percolator.json
COPY percolator-launch/app/lib/clawd-ids.js percolator-launch/app/lib/clawd-cluster.js /srv/app/lib/
COPY percolator-launch/app/components/clawd/ClawdChainApp.js /srv/app/components/clawd/ClawdChainApp.js
COPY percolator-launch/public /srv/public
COPY explorer/public/style.css explorer/public/logo.svg /srv/shared/
COPY extension/pow.js /srv/shared/pow.js
COPY deploy/hetzner/frontend/server.js /srv/server/clawd-frontend.js
COPY deploy/hetzner/frontend/perc-core.js deploy/hetzner/frontend/perc-api.js /srv/server/
ENV PORT=8080 \
    NETWORK=clawd \
    NEXT_PUBLIC_DEFAULT_NETWORK=clawd \
    CLAWD_RPC=http://validator:8080
EXPOSE 8080
CMD ["node", "/srv/server/clawd-frontend.js"]
