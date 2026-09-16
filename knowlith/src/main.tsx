import { StrictMode } from "react"
import { createRoot } from "react-dom/client"
// Shipped inside the binary. The page used to fetch these from Google on
// every open — the one request the product itself made to a server that is
// not the owner's — while the README said nothing leaves the machine.
import "@fontsource-variable/inter"
import "@fontsource/jetbrains-mono/400.css"
import "@fontsource/jetbrains-mono/500.css"
import "./index.css"
import { App } from "./App"

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
