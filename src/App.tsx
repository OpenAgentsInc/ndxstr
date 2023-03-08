import { useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import "./App.css";

function App() {
  const [greetMsg, setGreetMsg] = useState("");
  const [name, setName] = useState("wss://arc1.arcadelabs.co");

  async function greet() {
    // Learn more about Tauri commands at https://tauri.app/v1/guides/features/command
    setGreetMsg(await invoke("index_events", { relayurl: name } ))
    // setGreetMsg(await invoke("greet", { name }));
  }

  return (
    <div className="container">
      <h1>NDXSTR</h1>

      <p>Feed me Nostr events.</p>

      <div className="row">
        <form
          onSubmit={(e) => {
            e.preventDefault();
            greet();
          }}
        >
          <input
            id="greet-input"
            onChange={(e) => setName(e.currentTarget.value)}
            placeholder="Enter a relay URL..."
            value={name}
          />
          <button type="submit">Index</button>
        </form>
      </div>
      <p>{greetMsg}</p>
    </div>
  );
}

export default App;
