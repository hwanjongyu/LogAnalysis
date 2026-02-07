import { Plus, Trash2, Eye, EyeOff, Hash } from "lucide-react";

export interface LogFilter {
  id: string;
  pattern: string;
  is_include: boolean;
  is_enabled: boolean;
  color?: string;
}

interface FilterSidebarProps {
  filters: LogFilter[];
  onAddFilter: () => void;
  onUpdateFilter: (id: string, updates: Partial<LogFilter>) => void;
  onRemoveFilter: (id: string) => void;
}

const COLORS = [
  "#ef4444", "#f97316", "#f59e0b", "#10b981", 
  "#3b82f6", "#6366f1", "#8b5cf6", "#d946ef"
];

export function FilterSidebar({ 
  filters, 
  onAddFilter, 
  onUpdateFilter, 
  onRemoveFilter 
}: FilterSidebarProps) {
  return (
    <div className="flex flex-col h-full bg-card">
      <div className="p-4 border-b border-border flex justify-between items-center">
        <h2 className="font-bold text-lg">Filters</h2>
        <button 
          onClick={onAddFilter}
          className="p-1 hover:bg-accent rounded text-muted-foreground transition-colors"
          title="Add new filter"
        >
          <Plus size={18} />
        </button>
      </div>
      
      <div className="flex-1 overflow-y-auto p-3 space-y-3">
        {filters.length === 0 ? (
          <div className="p-4 text-sm text-muted-foreground italic text-center">
            No filters defined. Click + to add one.
          </div>
        ) : (
          filters.map((filter) => (
            <div 
              key={filter.id} 
              className={`p-3 rounded-lg border border-border bg-background/50 space-y-2 transition-all ${
                !filter.is_enabled ? "opacity-50" : ""
              }`}
            >
              <div className="flex items-center gap-2">
                <button
                  onClick={() => onUpdateFilter(filter.id, { is_enabled: !filter.is_enabled })}
                  className="text-muted-foreground hover:text-foreground"
                >
                  {filter.is_enabled ? <Eye size={16} /> : <EyeOff size={16} />}
                </button>
                
                <select
                  value={filter.is_include ? "include" : "exclude"}
                  onChange={(e) => onUpdateFilter(filter.id, { is_include: e.target.value === "include" })}
                  className="bg-accent border border-border text-[10px] px-1 py-0.5 rounded uppercase font-bold"
                >
                  <option value="include">Include</option>
                  <option value="exclude">Exclude</option>
                </select>

                <div className="flex-1" />
                
                <button
                  onClick={() => onRemoveFilter(filter.id)}
                  className="text-muted-foreground hover:text-destructive p-1 rounded hover:bg-destructive/10"
                >
                  <Trash2 size={14} />
                </button>
              </div>

              <input
                type="text"
                value={filter.pattern}
                onChange={(e) => onUpdateFilter(filter.id, { pattern: e.target.value })}
                placeholder="Regex pattern..."
                className="w-full bg-accent border border-border rounded px-2 py-1 text-xs focus:outline-none focus:ring-1 focus:ring-ring font-mono"
              />

              <div className="flex gap-1 overflow-x-auto pb-1">
                {COLORS.map(color => (
                  <button
                    key={color}
                    onClick={() => onUpdateFilter(filter.id, { color })}
                    className={`w-4 h-4 rounded-full border border-white/20 flex-shrink-0 ${
                      filter.color === color ? "ring-2 ring-primary ring-offset-1 ring-offset-background" : ""
                    }`}
                    style={{ backgroundColor: color }}
                  />
                ))}
                <button
                  onClick={() => onUpdateFilter(filter.id, { color: undefined })}
                  className={`w-4 h-4 rounded-full border border-border flex-shrink-0 bg-transparent flex items-center justify-center ${
                    !filter.color ? "ring-2 ring-primary ring-offset-1 ring-offset-background" : ""
                  }`}
                  title="No color"
                >
                  <X size={10} className="text-muted-foreground" />
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function X({ size, className }: { size: number, className: string }) {
  return (
    <svg 
      xmlns="http://www.w3.org/2000/svg" 
      width={size} 
      height={size} 
      viewBox="0 0 24 24" 
      fill="none" 
      stroke="currentColor" 
      strokeWidth="2" 
      strokeLinecap="round" 
      strokeLinejoin="round" 
      className={className}
    >
      <path d="M18 6 6 18"/><path d="m6 6 12 12"/>
    </svg>
  );
}
