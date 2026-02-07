import { useState, useEffect, useCallback } from "react";
import { 
  Settings, 
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
    <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground font-sans text-sm">
      {/* Sidebar */}
      <aside 
        className={`${
          isSidebarOpen ? "w-80" : "w-0"
        } transition-all duration-300 ease-in-out border-r border-border bg-card flex flex-col overflow-hidden`}
      >
        <FilterSidebar 
          filters={filters}
          onAddFilter={handleAddFilter}
          onUpdateFilter={handleUpdateFilter}
          onRemoveFilter={handleRemoveFilter}
        />

        <div className="p-4 border-t border-border mt-auto">
          <button className="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors w-full">
            <Settings size={16} />
            <span>Settings</span>
          </button>
        </div>
      </aside>

      {/* Main Content Area */}
      <main className="flex-1 flex flex-col min-w-0">
        {/* Header */}
        <header className="h-12 border-b border-border flex items-center px-4 justify-between bg-card">
          <div className="flex items-center gap-4">
            <button 
              onClick={() => setIsSidebarOpen(!isSidebarOpen)}
              className="p-1 hover:bg-accent rounded text-muted-foreground"
            >
              {isSidebarOpen ? <X size={20} /> : <Menu size={20} />}
            </button>
            <div className="flex gap-2">
              <button 
                onClick={handleOpenFile}
                className={`flex items-center gap-2 px-3 py-1 rounded border text-xs transition-colors ${
                  filePath ? "bg-primary/10 border-primary/20 text-primary" : "bg-accent/50 border-border text-muted-foreground hover:bg-accent"
                }`}
              >
                <FileText size={14} />
                <span className="max-w-[150px] truncate">
                  {filePath ? filePath.split("/").pop() : "Open File"}
                </span>
              </button>
              <button 
                onClick={handleToggleAdb}
                className={`flex items-center gap-2 px-3 py-1 rounded border text-xs transition-colors ${
                  isAdbActive ? "bg-green-500/10 border-green-500/20 text-green-600" : "bg-accent/50 border-border text-muted-foreground hover:bg-accent"
                }`}
              >
                <Smartphone size={14} />
                <span>ADB Logcat</span>
              </button>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <div className="relative">
              <Search className="absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground" size={14} />
              <input 
                type="text" 
                placeholder="Quick search..." 
                className="pl-8 pr-3 py-1 bg-accent border border-border rounded text-xs focus:outline-none focus:ring-1 focus:ring-ring w-64"
              />
            </div>
            <div className="h-6 w-[1px] bg-border mx-2"></div>
            <button 
              className="p-1.5 hover:bg-accent rounded text-muted-foreground"
              onClick={() => { setFilePath(null); setLineCount(0); setVisibleLineCount(0); if (isAdbActive) handleStopAdb(); }}
            >
              <Trash2 size={18} />
            </button>
            <button 
              onClick={handleToggleAdb}
              className={`p-1.5 rounded flex items-center gap-2 px-3 shadow-sm transition-all ${
                isAdbActive 
                ? "bg-destructive text-destructive-foreground hover:bg-destructive/90" 
                : "bg-primary text-primary-foreground hover:bg-primary/90"
              }`}
            >
              {isAdbActive ? <Square size={16} fill="currentColor" /> : <Play size={16} fill="currentColor" />}
              <span className="text-xs font-medium">{isAdbActive ? "Stop" : "Start"} ADB</span>
            </button>
          </div>
        </header>

        {/* Log View Area */}
        <div className="flex-1 overflow-hidden relative font-mono">
          {isIndexing && (
            <div className="absolute inset-0 z-10 bg-background/80 flex flex-col items-center justify-center gap-4">
              <Loader2 size={48} className="animate-spin text-primary" />
              <div className="w-64 h-2 bg-accent rounded-full overflow-hidden">
                <div 
                  className="h-full bg-primary transition-all duration-300" 
                  style={{ width: `${indexProgress}%` }}
                />
              </div>
              <p className="text-muted-foreground">Indexing log file... {indexProgress.toFixed(0)}%</p>
            </div>
          )}

          {isFiltering && !isAdbActive && (
            <div className="absolute top-4 right-4 z-20 bg-background/90 border border-border px-4 py-2 rounded-full shadow-lg flex items-center gap-3">
              <Loader2 size={16} className="animate-spin text-primary" />
              <span className="text-sm font-medium">Filtering...</span>
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
              className="absolute inset-0 flex items-center justify-center text-muted-foreground/30 flex-col gap-4 cursor-pointer hover:bg-accent/5 transition-colors"
              onClick={handleOpenFile}
            >
              <Filter size={64} strokeWidth={1} />
              <p className="text-xl font-sans">Open a log file or start ADB stream</p>
            </div>
          )}
        </div>

        {/* Status Bar */}
        <footer className="h-6 border-t border-border bg-card flex items-center px-3 text-[10px] text-muted-foreground justify-between">
          <div className="flex gap-4">
            <span>Total: {lineCount.toLocaleString()}</span>
            <span className={visibleLineCount !== lineCount ? "text-primary font-bold" : ""}>
              Visible: {visibleLineCount.toLocaleString()}
            </span>
          </div>
          <div className="flex gap-4 items-center">
            <span className={isAdbActive ? "text-green-500 animate-pulse font-bold" : ""}>
              ADB: {isAdbActive ? "Streaming" : "Disconnected"}
            </span>
            <div className="h-3 w-[1px] bg-border mx-1"></div>
            <div className="flex gap-2">
              <button onClick={() => handleZoom(-1)} className="hover:text-foreground">Zoom-</button>
              <span className="w-12 text-center">Scale: {((fontSize / 13) * 100).toFixed(0)}%</span>
              <button onClick={() => handleZoom(1)} className="hover:text-foreground">Zoom+</button>
            </div>
          </div>
        </footer>
      </main>
    </div>
  );
}

export default App;
