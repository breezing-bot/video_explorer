import { useEffect, useMemo, useRef, useState } from "react";
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

const DEFAULT_STATUS: ScanStatusDto = {
  isRunning: false,
  rootPath: null,
  totalCandidates: 0,
  scannedFiles: 0,
  hashedFiles: 0,
  errorCount: 0,
  lastError: null,
  startedAt: null,
  finishedAt: null,
};

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }

  const units = ["KB", "MB", "GB", "TB"];
  let size = bytes / 1024;
  let index = 0;

  while (size >= 1024 && index < units.length - 1) {
    size /= 1024;
    index += 1;
  }

  return `${size.toFixed(size >= 10 ? 0 : 1)} ${units[index]}`;
}

function App() {
  const [rootPath, setRootPath] = useState("");
  const [keyword, setKeyword] = useState("");
  const [duplicatesOnly, setDuplicatesOnly] = useState(true);
  const [hashRecords, setHashRecords] = useState<HashWithPaths[]>([]);
  const [status, setStatus] = useState<ScanStatusDto>(DEFAULT_STATUS);
  const [message, setMessage] = useState("Ready");
  const [isLoadingList, setIsLoadingList] = useState(false);
  const duplicatesOnlyRef = useRef(duplicatesOnly);

  const progressPercent = useMemo(() => {
    if (status.totalCandidates === 0) {
      return 0;
    }
    return Math.round((status.scannedFiles / status.totalCandidates) * 100);
  }, [status.scannedFiles, status.totalCandidates]);

  const filteredRecords = useMemo(() => {
    const ordered = [...hashRecords].sort((left, right) => {
      return right.occurrenceCount - left.occurrenceCount || left.fullHash.localeCompare(right.fullHash);
    });

    const value = keyword.trim().toLowerCase();
    if (!value) {
      return ordered;
    }

    return ordered.filter((entry) => {
      const hitHash = entry.fullHash.toLowerCase().includes(value);
      const hitPath = entry.paths.some((path) => path.toLowerCase().includes(value));
      return hitHash || hitPath;
    });
  }, [hashRecords, keyword]);

  useEffect(() => {
    duplicatesOnlyRef.current = duplicatesOnly;
  }, [duplicatesOnly]);

  useEffect(() => {
    void loadStatus();
  }, []);

  useEffect(() => {
    void loadHashRecords(duplicatesOnly);
  }, [duplicatesOnly]);

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
          void loadHashRecords(duplicatesOnlyRef.current);
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

  async function loadHashRecords(onlyDuplicates: boolean) {
    setIsLoadingList(true);
    try {
      const data = await getHashesWithPaths(onlyDuplicates);
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
    <main className="mx-auto grid max-w-7xl gap-4 pb-4 text-slate-900 md:gap-5 md:pb-6">
      <section className="rounded-2xl bg-gradient-to-r from-slate-950/90 via-slate-900/90 to-teal-900/85 px-5 py-4 text-white shadow-[0_14px_30px_rgba(8,24,48,0.32)] md:px-6 md:py-5">
        <h1 className="text-2xl font-semibold tracking-tight md:text-3xl">Video Hash Explorer</h1>
        <p className="mt-1.5 text-sm text-slate-200 md:text-base">
          Scan folders, compute content hashes, and keep the hash-to-path mapping synchronized in
          SQLite.
        </p>
      </section>

      <section className="panel p-4 md:p-5">
        <div className="mb-3 flex items-center justify-between gap-3">
          <h2 className="text-lg font-semibold text-slate-900">Scanner</h2>
          <span
            className={
              status.isRunning
                ? "rounded-full bg-teal-100 px-2.5 py-1 text-[11px] font-semibold uppercase tracking-[0.08em] text-teal-700"
                : "rounded-full bg-orange-100 px-2.5 py-1 text-[11px] font-semibold uppercase tracking-[0.08em] text-orange-700"
            }
          >
            {status.isRunning ? "Running" : "Idle"}
          </span>
        </div>

        <div className="grid gap-2.5 md:grid-cols-[1fr_auto_auto]">
          <input
            type="text"
            className="input-base"
            value={rootPath}
            onChange={(event) => setRootPath(event.currentTarget.value)}
            placeholder="Select or type a folder path"
          />
          <button type="button" className="btn-soft" onClick={chooseFolder}>
            Choose Folder
          </button>
          <button type="button" className="btn-primary" onClick={triggerScan} disabled={status.isRunning}>
            Start Scan
          </button>
        </div>

        <div className="mt-3 h-2 w-full overflow-hidden rounded-full bg-slate-200">
          <div
            className="h-full rounded-full bg-gradient-to-r from-amber-600 to-teal-500 transition-[width] duration-200"
            style={{ width: `${progressPercent}%` }}
          />
        </div>
        <div className="mt-3 grid grid-cols-2 gap-2.5 md:grid-cols-4">
          <div className="rounded-lg border border-slate-200 bg-slate-50 px-3 py-2">
            <span className="block text-[11px] uppercase tracking-[0.08em] text-slate-500">Total</span>
            <strong className="mt-0.5 block text-xl font-semibold leading-none">{status.totalCandidates}</strong>
          </div>
          <div className="rounded-lg border border-slate-200 bg-slate-50 px-3 py-2">
            <span className="block text-[11px] uppercase tracking-[0.08em] text-slate-500">Scanned</span>
            <strong className="mt-0.5 block text-xl font-semibold leading-none">{status.scannedFiles}</strong>
          </div>
          <div className="rounded-lg border border-slate-200 bg-slate-50 px-3 py-2">
            <span className="block text-[11px] uppercase tracking-[0.08em] text-slate-500">Hashed</span>
            <strong className="mt-0.5 block text-xl font-semibold leading-none">{status.hashedFiles}</strong>
          </div>
          <div className="rounded-lg border border-slate-200 bg-slate-50 px-3 py-2">
            <span className="block text-[11px] uppercase tracking-[0.08em] text-slate-500">Errors</span>
            <strong className="mt-0.5 block text-xl font-semibold leading-none">{status.errorCount}</strong>
          </div>
        </div>
        <p className="mt-3 text-sm text-slate-600">{message}</p>
      </section>

      <section className="panel p-4 md:p-5">
        <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
          <h2 className="text-lg font-semibold text-slate-900">Hash Map</h2>
          <div className="flex items-center gap-2">
            <label className="inline-flex cursor-pointer items-center gap-2 rounded-lg border border-slate-200 bg-white px-2.5 py-1.5 text-xs font-medium text-slate-700">
              <input
                type="checkbox"
                className="h-4 w-4 rounded border-slate-300 accent-teal-600"
                checked={duplicatesOnly}
                onChange={(event) => setDuplicatesOnly(event.currentTarget.checked)}
              />
              Only hashes with multiple paths
            </label>
            <button
              type="button"
              className="btn-soft"
              onClick={() => void loadHashRecords(duplicatesOnly)}
              disabled={isLoadingList}
            >
              {isLoadingList ? "Loading..." : "Refresh"}
            </button>
          </div>
        </div>

        <input
          type="text"
          className="input-base"
          placeholder="Filter by hash or path"
          value={keyword}
          onChange={(event) => setKeyword(event.currentTarget.value)}
        />

        <div className="mt-2 text-xs text-slate-600">
          Showing {filteredRecords.length} of {hashRecords.length} hash groups
        </div>

        <div className="mt-2 max-h-[520px] overflow-auto rounded-xl border border-slate-200 bg-white">
          <table className="min-w-full border-collapse text-left text-xs text-slate-800">
            <thead className="sticky top-0 z-10 bg-slate-100 text-[11px] uppercase tracking-[0.08em] text-slate-600">
              <tr>
                <th className="w-[32%] px-3 py-2 font-semibold">Hash</th>
                <th className="w-[10%] px-3 py-2 font-semibold">Count</th>
                <th className="w-[12%] px-3 py-2 font-semibold">Size</th>
                <th className="w-[46%] px-3 py-2 font-semibold">Paths</th>
              </tr>
            </thead>
            <tbody>
              {filteredRecords.map((record) => (
                <tr key={record.fullHash} className="border-t border-slate-100 align-top hover:bg-slate-50/70">
                  <td className="px-3 py-2 font-mono text-[11px] leading-5 text-slate-800">
                    <div className="break-all">{record.fullHash}</div>
                    <div className="mt-1 break-all text-[10px] text-slate-500">
                      fp: {record.fingerprintHash}
                    </div>
                  </td>
                  <td className="px-3 py-2 text-sm font-semibold text-slate-900">{record.occurrenceCount}</td>
                  <td className="px-3 py-2 text-sm text-slate-700">{formatBytes(record.fileSize)}</td>
                  <td className="px-3 py-2 text-[11px] text-slate-700">
                    <div className="break-all font-mono text-[10px] text-slate-500">{record.paths[0]}</div>
                    <details className="mt-1">
                      <summary className="cursor-pointer select-none text-[11px] font-semibold text-teal-700">
                        {record.paths.length > 1
                          ? `Show all ${record.paths.length} paths`
                          : "Show path details"}
                      </summary>
                      <ul className="mt-1 space-y-1">
                        {record.paths.map((path) => (
                          <li key={`${record.fullHash}-${path}`} className="break-all font-mono text-[10px] text-slate-700">
                            {path}
                          </li>
                        ))}
                      </ul>
                    </details>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>

          {!filteredRecords.length && (
            <p className="px-3 py-4 text-sm text-slate-500">No hash record found for current filters.</p>
          )}
        </div>
      </section>
    </main>
  );
}

export default App;
