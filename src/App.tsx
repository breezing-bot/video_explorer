import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

import { getHashesWithPaths, getScanStatus, startScan } from "./services/tauriApi";
import {
  HashWithPaths,
  ScanCompletedEvent,
  ScanErrorEvent,
  ScanProgressEvent,
  ScanStartedEvent,
  ScanStatusDto,
} from "./types";
import "./App.css";

function App() {
  const [rootPath, setRootPath] = useState("");
  const [keyword, setKeyword] = useState("");
  const [hashRecords, setHashRecords] = useState<HashWithPaths[]>([]);
  const [status, setStatus] = useState<ScanStatusDto>({
    isRunning: false,
    rootPath: null,
    totalCandidates: 0,
    scannedFiles: 0,
    hashedFiles: 0,
    errorCount: 0,
    lastError: null,
    startedAt: null,
    finishedAt: null,
  });
  const [message, setMessage] = useState("Ready");
  const [isLoadingList, setIsLoadingList] = useState(false);

  const progressPercent = useMemo(() => {
    if (status.totalCandidates === 0) {
      return 0;
    }
    return Math.round((status.scannedFiles / status.totalCandidates) * 100);
  }, [status.scannedFiles, status.totalCandidates]);

  const filteredRecords = useMemo(() => {
    const value = keyword.trim().toLowerCase();
    if (!value) {
      return hashRecords;
    }

    return hashRecords.filter((entry) => {
      const hitHash = entry.fullHash.toLowerCase().includes(value);
      const hitPath = entry.paths.some((path) => path.toLowerCase().includes(value));
      return hitHash || hitPath;
    });
  }, [hashRecords, keyword]);

  useEffect(() => {
    void loadStatus();
    void loadHashRecords();
  }, []);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];

    const setupListeners = async () => {
      unlisteners.push(
        await listen<ScanStartedEvent>("scan:started", (event) => {
          setStatus((current) => ({
            ...current,
            isRunning: true,
            rootPath: event.payload.rootPath,
            totalCandidates: event.payload.totalCandidates,
            scannedFiles: 0,
            hashedFiles: 0,
            errorCount: 0,
            lastError: null,
            startedAt: new Date().toISOString(),
            finishedAt: null,
          }));
          setMessage(`Scanning started: ${event.payload.rootPath}`);
        }),
      );

      unlisteners.push(
        await listen<ScanProgressEvent>("scan:progress", (event) => {
          setStatus((current) => ({
            ...current,
            isRunning: true,
            scannedFiles: event.payload.scannedFiles,
            totalCandidates: event.payload.totalCandidates,
            hashedFiles: event.payload.hashedFiles,
            errorCount: event.payload.errorCount,
          }));
          setMessage(`Scanning: ${event.payload.path}`);
        }),
      );

      unlisteners.push(
        await listen<ScanCompletedEvent>("scan:completed", (event) => {
          setStatus((current) => ({
            ...current,
            isRunning: false,
            scannedFiles: event.payload.scannedFiles,
            hashedFiles: event.payload.hashedFiles,
            errorCount: event.payload.errorCount,
            finishedAt: event.payload.finishedAt,
          }));
          setMessage(
            `Scan finished. Scanned ${event.payload.scannedFiles}, hashed ${event.payload.hashedFiles}, removed ${event.payload.removedPaths}`,
          );
          void loadHashRecords();
        }),
      );

      unlisteners.push(
        await listen<ScanErrorEvent>("scan:error", (event) => {
          setStatus((current) => ({
            ...current,
            errorCount: current.errorCount + 1,
            lastError: event.payload.message,
          }));
          setMessage(`Error: ${event.payload.message}`);
        }),
      );
    };

    void setupListeners();

    return () => {
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, []);

  const chooseFolder = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Select video folder",
    });

    if (typeof selected === "string") {
      setRootPath(selected);
    }
  };

  const triggerScan = async () => {
    if (!rootPath.trim()) {
      setMessage("Please select a folder first.");
      return;
    }

    try {
      await startScan(rootPath);
      setMessage("Scan task queued.");
    } catch (error) {
      setMessage(String(error));
    }
  };

  async function loadHashRecords() {
    setIsLoadingList(true);
    try {
      const data = await getHashesWithPaths();
      setHashRecords(data);
    } catch (error) {
      setMessage(`Query failed: ${String(error)}`);
    } finally {
      setIsLoadingList(false);
    }
  }

  async function loadStatus() {
    try {
      const data = await getScanStatus();
      setStatus(data);
      if (data.rootPath) {
        setRootPath(data.rootPath);
      }
    } catch (error) {
      setMessage(`Status fetch failed: ${String(error)}`);
    }
  }

  return (
    <main className="app-shell">
      <section className="hero-card">
        <h1>Video Hash Explorer</h1>
        <p>
          Scan folders, compute content hashes, and keep the hash-to-path mapping synchronized in
          SQLite.
        </p>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h2>Scanner</h2>
          <span className={status.isRunning ? "badge badge-running" : "badge badge-idle"}>
            {status.isRunning ? "Running" : "Idle"}
          </span>
        </div>

        <div className="scanner-row">
          <input
            type="text"
            className="path-input"
            value={rootPath}
            onChange={(event) => setRootPath(event.currentTarget.value)}
            placeholder="Select or type a folder path"
          />
          <button type="button" onClick={chooseFolder}>
            Choose Folder
          </button>
          <button type="button" onClick={triggerScan} disabled={status.isRunning}>
            Start Scan
          </button>
        </div>

        <div className="progress-track">
          <div className="progress-fill" style={{ width: `${progressPercent}%` }} />
        </div>
        <div className="stat-grid">
          <div>
            <span>Total</span>
            <strong>{status.totalCandidates}</strong>
          </div>
          <div>
            <span>Scanned</span>
            <strong>{status.scannedFiles}</strong>
          </div>
          <div>
            <span>Hashed</span>
            <strong>{status.hashedFiles}</strong>
          </div>
          <div>
            <span>Errors</span>
            <strong>{status.errorCount}</strong>
          </div>
        </div>
        <p className="message-line">{message}</p>
      </section>

      <section className="panel">
        <div className="panel-header">
          <h2>Hash Map</h2>
          <button type="button" onClick={() => void loadHashRecords()} disabled={isLoadingList}>
            {isLoadingList ? "Loading..." : "Refresh"}
          </button>
        </div>
        <input
          type="text"
          className="filter-input"
          placeholder="Filter by hash or path"
          value={keyword}
          onChange={(event) => setKeyword(event.currentTarget.value)}
        />
        <div className="result-count">{filteredRecords.length} hash groups</div>
        <div className="result-list">
          {filteredRecords.map((record) => (
            <article key={record.fullHash} className="hash-item">
              <header>
                <h3>{record.fullHash}</h3>
                <span>{record.occurrenceCount} path(s)</span>
              </header>
              <p className="fingerprint">fingerprint: {record.fingerprintHash}</p>
              <ul>
                {record.paths.map((path) => (
                  <li key={`${record.fullHash}-${path}`}>{path}</li>
                ))}
              </ul>
            </article>
          ))}
          {!filteredRecords.length && <p className="empty-state">No hash record found.</p>}
        </div>
      </section>
    </main>
  );
}

export default App;
