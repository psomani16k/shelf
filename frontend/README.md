# Shelf frontend

The React + Vite + TypeScript UI, shared by the web build (served by `shelf-server`) and the Tauri app. How it's put together is in [docs/frontend.md](../docs/frontend.md); setting up a development environment is in [CONTRIBUTING.md](../CONTRIBUTING.md).

```sh
pnpm --dir frontend dev       # Vite dev server; /api goes to localhost:8080
pnpm --dir frontend build     # tsc, then the production build into dist/
pnpm --dir frontend test      # vitest
pnpm --dir frontend lint      # oxlint
```

`dev`, `build` and `test` regenerate the proto types into `src/gen/` first (`buf generate`). Don't edit `src/gen/` or `vendor/foliate-js`.
