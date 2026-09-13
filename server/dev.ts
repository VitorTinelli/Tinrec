import "dotenv/config";
import { createServer } from "vite";
import { createApp } from "./app.js";
const app = createApp();
const vite = await createServer({
  server: { middlewareMode: true },
  appType: "spa",
});
app.use(vite.middlewares);
app.listen(5173, "127.0.0.1", () =>
  console.log("Tinrec: http://localhost:5173"),
);
