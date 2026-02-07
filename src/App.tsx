import { useState, useEffect, useCallback } from "react";
import {
  Search,
  Menu,
  X,
  FileText,
  Play,
  Square,
  Trash2,
  Filter,
  Loader2,
  Smartphone
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LogViewer } from "./components/LogViewer";
import { FilterSidebar, LogFilter } from "./components/FilterSidebar";

function App() {
  const [isSidebarOpen, setIsSidebarOpen] = useState(true);
  const [filePath, setFilePath] = useState<string | null>(null);
  const [lineCount, setLineCount] = useState(0);
  const [visibleLineCount, setVisibleLineCount] = useState(0);
  const [isIndexing, setIsIndexing] = useState(false);
  const [isFiltering, setIsFiltering] = useState(false);
  const [isAdbActive, setIsAdbActive] = useState(false);
  const [indexProgress, setIndexProgress] = useState(0);
  const [fontSize, setFontSize] = useState(13);
  const [filters, setFilters] = useState<LogFilter[]>([]);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  useEffect(() => {
    const unlistenIndex = listen<{ progress: number }>("indexing-progress", (event) => {
      setIndexProgress(event.payload.progress * 100);
    });

    const unlistenAdb = listen<string[]>("adb-new-lines", (event) => {
      setLineCount((prev) => {
        const next = prev + event.payload.length;
        setVisibleLineCount(next);
        return next;
      });
    });

    return () => {
      unlistenIndex.then((fn) => fn());
      unlistenAdb.then((fn) => fn());
    };
  }, []);

  const handleApplyFilters = useCallback(async (currentFilters: LogFilter[]) => {
    if (!filePath && !isAdbActive) return;

    // If ADB is active, we don't apply filters to existing lines in this MVP.
    // Filters are applied by the backend during ingestion.
    if (isAdbActive) return;

    setIsFiltering(true);
    try {
      const count = await invoke<number>("apply_filters", { filters: currentFilters });
      setVisibleLineCount(count);
    } catch (error) {
      console.error("Failed to apply filters:", error);
    } finally {
      setIsFiltering(false);
    }
  }, [filePath, isAdbActive]);

  useEffect(() => {
    const timer = setTimeout(() => {
      handleApplyFilters(filters);
    }, 500);
    return () => clearTimeout(timer);
  }, [filters, handleApplyFilters]);

  const handleOpenFile = async () => {
    setErrorMessage(null);
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Logs", extensions: ["log", "txt", "out"] }]
      });

      if (selected && typeof selected === "string") {
        if (isAdbActive) await handleStopAdb();

        setFilePath(selected);
        setIsIndexing(true);
        setIndexProgress(0);

        const count = await invoke<number>("open_file", { path: selected });
        setLineCount(count);
        setVisibleLineCount(count);
        setIsIndexing(false);
      }
    } catch (error) {
      console.error("Failed to open file:", error);
      setErrorMessage(String(error));
      setIsIndexing(false);
    }
  };

  const handleToggleAdb = async () => {
    setErrorMessage(null);
    if (isAdbActive) {
      await handleStopAdb();
    } else {
      await handleStartAdb();
    }
  };

  const handleStartAdb = async () => {
    try {
      setFilePath(null);
      setLineCount(0);
      setVisibleLineCount(0);
      await invoke("start_adb", { filters });
      setIsAdbActive(true);
    } catch (error) {
      console.error("Failed to start ADB:", error);
      setErrorMessage(String(error));
    }
  };

  const handleStopAdb = async () => {
    try {
      await invoke("stop_adb");
      setIsAdbActive(false);
    } catch (error) {
      console.error("Failed to stop ADB:", error);
      setErrorMessage(String(error));
    }
  };

  const handleAddFilter = () => {
    const newFilter: LogFilter = {
      id: crypto.randomUUID(),
      pattern: "",
      is_include: true,
      is_enabled: true,
    };
    setFilters([...filters, newFilter]);
  };

  const handleUpdateFilter = (id: string, updates: Partial<LogFilter>) => {
    setFilters(filters.map(f => f.id === id ? { ...f, ...updates } : f));
  };

  const handleRemoveFilter = (id: string) => {
    setFilters(filters.filter(f => f.id !== id));
  };

  const handleZoom = (delta: number) => {
    setFontSize((prev) => Math.min(Math.max(8, prev + delta), 32));
  };

  return (
    <div className="flex flex-col h-screen w-screen overflow-hidden bg-background text-foreground font-sans text-sm">
      {/* Header */}
      <header className="h-12 border-b border-border/50 flex items-center px-4 justify-between bg-card/80 backdrop-blur-xl shrink-0 shadow-md z-30">
        <div className="flex items-center gap-4">
          <button
            onClick={() => setIsSidebarOpen(!isSidebarOpen)}
            className="p-1.5 hover:bg-accent rounded-lg text-muted-foreground transition-all duration-fast hover:scale-110 active:scale-90"
          >
            {isSidebarOpen ? <X size={20} /> : <Menu size={20} />}
          </button>
          <div className="flex gap-2">
            <button
              onClick={handleOpenFile}
              className={`
                flex items-center gap-2 px-3 py-1.5 rounded-lg border text-xs 
                transition-all duration-fast hover:scale-[1.02] active:scale-[0.98]
                ${filePath
                  ? "bg-primary/10 border-primary/40 text-primary shadow-glow"
                  : "bg-accent/50 border-border text-muted-foreground hover:bg-accent hover:shadow-md"
                }
              `}
            >
              <FileText size={14} />
              <span className="max-w-[150px] truncate font-medium">
                {filePath ? filePath.split("/").pop() : "Open File"}
              </span>
            </button>
            <button
              onClick={handleToggleAdb}
              className={`
                flex items-center gap-2 px-3 py-1.5 rounded-lg border text-xs 
                transition-all duration-fast hover:scale-[1.02] active:scale-[0.98]
                ${isAdbActive
                  ? "bg-success/10 border-success/40 text-success shadow-[0_0_15px_rgba(16,185,129,0.2)]"
                  : "bg-accent/50 border-border text-muted-foreground hover:bg-accent hover:shadow-md"
                }
              `}
            >
              <Smartphone size={14} />
              <span className="font-medium">ADB Logcat</span>
            </button>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <div className="relative group">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground transition-colors group-focus-within:text-primary" size={14} />
            <input
              type="text"
              placeholder="Quick search..."
              className="
                pl-9 pr-3 py-1.5 bg-accent/50 border border-border rounded-lg text-xs 
                transition-all duration-fast
                focus:outline-none focus:ring-2 focus:ring-primary/50 focus:border-primary focus:bg-accent
                hover:border-border/80
                w-64
              "
            />
          </div>
          <div className="h-6 w-[1px] bg-border/50 mx-2"></div>
          <button
            className="p-2 hover:bg-destructive/10 hover:text-destructive rounded-lg text-muted-foreground transition-all duration-fast hover:scale-110 active:scale-90"
            onClick={() => { setFilePath(null); setLineCount(0); setVisibleLineCount(0); if (isAdbActive) handleStopAdb(); }}
            title="Clear all"
          >
            <Trash2 size={18} />
          </button>
          <button
            onClick={handleToggleAdb}
            className={`
              p-1.5 rounded-lg flex items-center gap-2 px-4 shadow-md transition-all duration-fast
              hover:scale-[1.02] active:scale-[0.95]
              ${isAdbActive
                ? "bg-destructive text-destructive-foreground hover:bg-destructive/90"
                : "bg-primary text-primary-foreground hover:bg-primary/90 shadow-glow"
              }
            `}
          >
            {isAdbActive ? <Square size={16} fill="currentColor" /> : <Play size={16} fill="currentColor" />}
            <span className="text-xs font-semibold">{isAdbActive ? "Stop" : "Start"} ADB</span>
          </button>
        </div>
      </header>

      {/* Main Content Area (Log View Area) */}
      <main className="flex-1 overflow-hidden relative font-mono">
        {isIndexing && (
          <div className="absolute inset-0 z-40 bg-background/90 backdrop-blur-sm flex flex-col items-center justify-center gap-6">
            <Loader2 size={52} className="animate-spin text-primary drop-shadow-[0_0_8px_rgba(250,95,235,0.4)]" />
            <div className="w-80 space-y-3">
              <div className="h-2.5 bg-accent/50 rounded-full overflow-hidden shadow-inner border border-white/5">
                <div
                  className="h-full bg-gradient-to-r from-primary via-purple-500 to-primary bg-[length:200%_100%] animate-shimmer transition-all duration-300 shadow-md"
                  style={{ width: `${indexProgress}%` }}
                />
              </div>
              <p className="text-muted-foreground text-center text-sm font-medium animate-pulse">
                Indexing log file...
                <span className="text-primary font-bold ml-2 tabular-nums">{indexProgress.toFixed(0)}%</span>
              </p>
            </div>
          </div>
        )}

        {isFiltering && !isAdbActive && (
          <div className="absolute top-4 right-4 z-50 bg-card/80 backdrop-blur-md border border-primary/20 px-4 py-2 rounded-full shadow-glow flex items-center gap-3 animate-in fade-in slide-in-from-top-2 duration-300">
            <Loader2 size={16} className="animate-spin text-primary" />
            <span className="text-xs font-bold tracking-tight text-primary">FILTERING...</span>
          </div>
        )}

        {errorMessage && (
          <div className="absolute top-4 left-1/2 -translate-x-1/2 z-50 bg-destructive text-destructive-foreground px-4 py-2 rounded shadow-lg flex items-center gap-3 max-w-[80%]">
            <X size={16} className="cursor-pointer shrink-0" onClick={() => setErrorMessage(null)} />
            <span className="text-sm font-medium truncate">Error: {errorMessage}</span>
          </div>
        )}

        {filePath || isAdbActive ? (
          <LogViewer
            key={filePath || "adb-stream"}
            filePath={filePath || "ADB_STREAM"}
            lineCount={visibleLineCount}
            fontSize={fontSize}
            filters={filters}
          />
        ) : (
          <div
            className="absolute inset-0 flex items-center justify-center text-muted-foreground/20 flex-col gap-6 cursor-pointer hover:bg-accent/5 transition-all group duration-slow"
            onClick={handleOpenFile}
          >
            <div className="relative">
              <Filter size={80} strokeWidth={0.5} className="group-hover:scale-110 group-hover:text-primary/30 transition-all duration-slow" />
              <div className="absolute inset-0 bg-primary/5 blur-3xl rounded-full scale-150 opacity-0 group-hover:opacity-100 transition-opacity duration-slow" />
            </div>
            <div className="text-center space-y-2 animate-in fade-in slide-in-from-bottom-4 duration-slow">
              <p className="text-2xl font-bold tracking-tight text-muted-foreground/40 group-hover:text-primary/40 transition-colors">Start Your Analysis</p>
              <p className="text-sm font-medium text-muted-foreground/30 group-hover:text-muted-foreground/40 transition-colors italic">Select a log file or start an ADB stream to begin</p>
            </div>
          </div>
        )}
      </main>

      {/* Filter Panel (Bottom) */}
      <aside
        className={`
          ${isSidebarOpen ? "h-72" : "h-0"} 
          transition-all duration-slow ease-in-out 
          border-t border-border/50 bg-card/80 backdrop-blur-md
          flex flex-col overflow-hidden shrink-0
          shadow-[0_-4px_12px_rgba(0,0,0,0.3)]
          z-20
        `}
      >
        <FilterSidebar
          filters={filters}
          onAddFilter={handleAddFilter}
          onUpdateFilter={handleUpdateFilter}
          onRemoveFilter={handleRemoveFilter}
        />
      </aside>

      {/* Status Bar */}
      <footer className="
        h-6 border-t border-border/50 bg-card/90 backdrop-blur-sm
        flex items-center px-3 text-[10px] text-muted-foreground justify-between shrink-0
        shadow-[0_-1px_3px_rgba(0,0,0,0.2)]
        z-30
      ">
        <div className="flex gap-4 items-center">
          <span className="flex items-center gap-1.5 transition-all duration-base">
            <span className="text-muted-foreground/60">Total:</span>
            <span className="font-semibold text-foreground tabular-nums">{lineCount.toLocaleString()}</span>
          </span>
          <div className="h-2.5 w-[1px] bg-border/40"></div>
          <span className={`flex items-center gap-1.5 transition-all duration-base ${visibleLineCount !== lineCount ? "text-primary font-bold drop-shadow-[0_0_4px_rgba(250,95,235,0.3)]" : ""}`}>
            <span className="text-muted-foreground/60">Visible:</span>
            <span className="font-semibold tabular-nums">{visibleLineCount.toLocaleString()}</span>
          </span>
        </div>

        <div className="flex gap-4 items-center">
          <span className={`flex items-center gap-2 transition-all duration-base ${isAdbActive ? "text-success font-semibold" : ""}`}>
            {isAdbActive && (
              <span className="relative flex h-2 w-2">
                <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-success opacity-75"></span>
                <span className="relative inline-flex rounded-full h-2 w-2 bg-success"></span>
              </span>
            )}
            <span className="text-muted-foreground/60">ADB:</span>
            {isAdbActive ? "Streaming" : "Disconnected"}
          </span>

          <div className="h-3 w-[1px] bg-border/70 mx-1"></div>

          <div className="flex gap-1.5 items-center">
            <button
              onClick={() => handleZoom(-1)}
              className="hover:text-foreground hover:bg-accent/50 transition-all duration-fast px-1.5 py-0.5 rounded-md hover:scale-110 active:scale-90"
              title="Zoom out"
            >
              −
            </button>
            <span className="w-12 text-center tabular-nums font-medium text-foreground/80">
              {((fontSize / 13) * 100).toFixed(0)}%
            </span>
            <button
              onClick={() => handleZoom(1)}
              className="hover:text-foreground hover:bg-accent/50 transition-all duration-fast px-1.5 py-0.5 rounded-md hover:scale-110 active:scale-90"
              title="Zoom in"
            >
              +
            </button>
          </div>
        </div>
      </footer>
    </div>
  );
}

export default App;
