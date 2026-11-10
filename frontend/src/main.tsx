import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
import "./index.css"
import App from "./App.tsx"
import { installErrorHandlers } from "./lib/log.ts"
import { IS_TAURI } from "./lib/platform.ts"
import { loadUiScale } from "./lib/uiScale.ts"
import { revokeCoversOnRemoval } from "./lib/useCover.ts"

installErrorHandlers()

const queryClient = new QueryClient()
revokeCoversOnRemoval(queryClient)
if (IS_TAURI) await loadUiScale(queryClient)

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
)
