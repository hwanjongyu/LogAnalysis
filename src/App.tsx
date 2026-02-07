import { useState } from "react";
import { 
  Plus, 
  Settings, 
  Search, 
  Menu, 
  X, 
  FileText, 
  Play, 
  Trash2, 
  Filter
} from "lucide-react";

function App() {
  const [isSidebarOpen, setIsSidebarOpen] = useState(true);

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
          {/* Placeholder for filter items */}
          <div className="p-2 text-sm text-muted-foreground italic">
            No filters active
          </div>
        </div>

        <div className="p-4 border-t border-border mt-auto">
          <button className="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground transition-colors">
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
            <div className="flex items-center gap-2 px-3 py-1 bg-accent/50 rounded border border-border text-sm">
              <FileText size={14} className="text-muted-foreground" />
              <span className="max-w-[200px] truncate">example.log</span>
            </div>
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
            <button className="p-1.5 hover:bg-accent rounded text-muted-foreground title='Clear Logs'">
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
          {/* This is where react-virtuoso will go */}
          <div className="absolute inset-0 flex items-center justify-center text-muted-foreground/30 flex-col gap-4">
            <Filter size={64} strokeWidth={1} />
            <p className="text-xl">Drop a log file here or start ADB stream</p>
          </div>
        </div>

        {/* Status Bar */}
        <footer className="h-6 border-t border-border bg-card flex items-center px-3 text-[10px] text-muted-foreground justify-between">
          <div className="flex gap-4">
            <span>Total: 0 lines</span>
            <span>Visible: 0 lines</span>
          </div>
          <div className="flex gap-4">
            <span>ADB: Disconnected</span>
            <span>Scale: 100%</span>
          </div>
        </footer>
      </main>
    </div>
  );
}

export default App;