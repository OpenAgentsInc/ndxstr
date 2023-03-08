import { useState } from "react";
import { invoke } from "@tauri-apps/api/tauri";
import "./App.css";
import { useInterval } from "./hooks";

function App() {
    const [greetMsg, setGreetMsg] = useState("");
    const [eventCount, setEventCount] = useState(0);
    const [name, setName] = useState("wss://arc1.arcadelabs.co");

    // async function buildRelayList() {
    //     await invoke("build_relay_list")
    // }

    async function fetchEventsCount() {
        const count = await invoke("fetch_events_count") as number
        console.log("Fetched events count: ", count)
        setEventCount(count)
    }

    useInterval(() => {
        fetchEventsCount()
    }, 1000)

    async function greet() {
        setGreetMsg(`Indexing ${name}...`)
        invoke("index_events", { relayurl: name })
    }

    return (
        <div className="container">
            <h1>NDXSTR</h1>

            <p style={{ fontStyle: 'italic' }}>Feed me Nostr events!</p>

            <p>Indexed events</p>
            <p style={{ fontSize: 24, marginTop: -16 }}><strong>{eventCount}</strong></p>

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
