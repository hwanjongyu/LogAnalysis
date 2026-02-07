import { useState, useEffect } from "react";
import { 
  Plus, 
  Settings, 
  Search, 
  Menu, 
  X, 
  FileText, 
  Play, 
  Trash2, 
  Filter,
  Loader2
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LogViewer } from "./components/LogViewer";

function App() {
  const [isSidebarOpen, setIsSidebarOpen] = useState(true);
  const [filePath, setFilePath] = useState<string | null>(null);
  const [lineCount, setLineCount] = useState(0);
  const [isIndexing, setIsIndexing] = useState(false);
  const [indexProgress, setIndexProgress] = useState(0);
  const [fontSize, setFontSize] = useState(13);

  useEffect(() => {
    const unlisten = listen<{ progress: number }>("indexing-progress", (event) => {
      setIndexProgress(event.payload.progress * 100);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const handleOpenFile = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [{ name: "Logs", extensions: ["log", "txt", "out"] }]
      });

      if (selected && typeof selected === "string") {
        setFilePath(selected);
        setIsIndexing(true);
        setIndexProgress(0);
        
        const count = await invoke<number>("open_file", { path: selected });
        setLineCount(count);
        setIsIndexing(false);
      }
    } catch (error) {
      console.error("Failed to open file:", error);
      setIsIndexing(false);
    }
  };

  const handleZoom = (delta: number) => {
    setFontSize((prev) => Math.min(Math.max(8, prev + delta), 32));
  };

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey) {
        if (e.key === "=" || e.key === "+") {
          e.preventDefault();
          handleZoom(1);
        } else if (e.key === "-") {
          e.preventDefault();
          handleZoom(-1);
        } else if (e.key === "0") {
          e.preventDefault();
          setFontSize(13);
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground font-sans">
      {/* Sidebar */}
      <aside 
        className={`${
          isSidebarOpen ? "w-64" : "w-0"
        } transition-all duration-300 ease-in-out border-r border-border bg-card flex flex-col`}
      >
        <div className="p-4 border-b border-border flex justify-between items-center overflow-hidden">
          <h2 className="font-bold text-lg whitespace-nowrap">Filters</h2>
          <button className="p-1 hover:bg-accent rounded text-muted-foreground">
            <Plus size={18} />
          </button>
        </div>
        
        <div className="flex-1 overflow-y-auto p-2 space-y-2">
          <div className="p-2 text-sm text-muted-foreground italic">
            No filters active
          </div>
        </div>

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
            <button 
              onClick={handleOpenFile}
              className="flex items-center gap-2 px-3 py-1 bg-accent/50 hover:bg-accent rounded border border-border text-sm transition-colors"
            >
              <FileText size={14} className="text-muted-foreground" />
              <span className="max-w-[200px] truncate">
                {filePath ? filePath.split("/").pop() : "Open Log File..."}
              </span>
            </button>
          </div>

          <div className="flex items-center gap-2">
            <div className="relative">
              <Search className="absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground" size={14} />
              <input 
                type="text" 
                placeholder="Quick search..." 
                className="pl-8 pr-3 py-1 bg-accent border border-border rounded text-sm focus:outline-none focus:ring-1 focus:ring-ring w-64"
              />
            </div>
            <div className="h-6 w-[1px] bg-border mx-2"></div>
            <button 
              className="p-1.5 hover:bg-accent rounded text-muted-foreground"
              onClick={() => { setFilePath(null); setLineCount(0); }}
            >
              <Trash2 size={18} />
            </button>
            <button className="p-1.5 bg-primary text-primary-foreground hover:bg-primary/90 rounded flex items-center gap-2 px-3">
              <Play size={16} />
              <span className="text-sm font-medium">ADB Logcat</span>
            </button>
          </div>
        </header>

        {/* Log View Area */}
        <div className="flex-1 overflow-hidden relative font-mono text-sm">
          {isIndexing ? (
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
          ) : filePath ? (
            <LogViewer filePath={filePath} lineCount={lineCount} fontSize={fontSize} />
          ) : (
            <div 
              className="absolute inset-0 flex items-center justify-center text-muted-foreground/30 flex-col gap-4 cursor-pointer hover:bg-accent/5 transition-colors"
              onClick={handleOpenFile}
            >
              <Filter size={64} strokeWidth={1} />
              <p className="text-xl">Open a log file or start ADB stream</p>
            </div>
          )}
        </div>

        {/* Status Bar */}
        <footer className="h-6 border-t border-border bg-card flex items-center px-3 text-[10px] text-muted-foreground justify-between">
          <div className="flex gap-4">
            <span>Total: {lineCount.toLocaleString()} lines</span>
            <span>Visible: {lineCount.toLocaleString()} lines</span>
          </div>
          <div className="flex gap-4">
            <span>ADB: Disconnected</span>
            <div className="flex gap-2">
              <button onClick={() => handleZoom(-1)} className="hover:text-foreground">Zoom-</button>
              <span>Scale: {((fontSize / 13) * 100).toFixed(0)}%</span>
              <button onClick={() => handleZoom(1)} className="hover:text-foreground">Zoom+</button>
            </div>
          </div>
        </footer>
      </main>
    </div>
  );
}

export default App;
