import { Plus, Trash2, Eye, EyeOff } from "lucide-react";

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
    <div className="flex h-full bg-transparent overflow-hidden">
      <div className="p-4 border-r border-border/50 flex flex-col justify-between items-center w-32 shrink-0 bg-accent/20 backdrop-blur-sm">
        <h2 className="font-bold text-xs uppercase tracking-widest text-muted-foreground/80 text-center">Filters</h2>
        <button
          onClick={onAddFilter}
          className="p-3 bg-primary text-primary-foreground hover:bg-primary/90 rounded-2xl transition-all duration-fast shadow-glow hover:scale-110 active:scale-95"
          title="Add new filter"
        >
          <Plus size={24} />
        </button>
      </div>

      <div className="flex-1 overflow-x-auto p-6 flex gap-6 items-start scroll-smooth">
        {filters.length === 0 ? (
          <div className="h-full flex items-center justify-center flex-1 text-sm text-muted-foreground/50 font-medium italic">
            No filters defined. Click + to add one.
          </div>
        ) : (
          filters.map((filter) => (
            <div
              key={filter.id}
              className={`
                p-4 rounded-2xl border border-border/50 bg-card/50 space-y-3 w-64 shrink-0
                transition-all duration-base
                hover:translate-y-[-4px] hover:shadow-xl hover:border-primary/30 hover:bg-card
                shadow-lg
                ${!filter.is_enabled ? "opacity-40 grayscale-[0.5] hover:opacity-70 hover:grayscale-0" : ""}
              `}
            >
              <div className="flex items-center gap-2">
                <button
                  onClick={() => onUpdateFilter(filter.id, { is_enabled: !filter.is_enabled })}
                  className={`p-1.5 rounded-lg transition-colors ${filter.is_enabled ? "text-primary hover:bg-primary/10" : "text-muted-foreground hover:bg-accent"}`}
                  title={filter.is_enabled ? "Disable filter" : "Enable filter"}
                >
                  {filter.is_enabled ? <Eye size={16} /> : <EyeOff size={16} />}
                </button>

                <select
                  value={filter.is_include ? "include" : "exclude"}
                  onChange={(e) => onUpdateFilter(filter.id, { is_include: e.target.value === "include" })}
                  className={`
                    bg-accent/70 border border-border/70 text-[10px] px-2 py-1 rounded-md uppercase font-black tracking-tighter
                    transition-all duration-fast cursor-pointer focus:outline-none focus:ring-2 focus:ring-primary/30
                    ${filter.is_include ? "text-success" : "text-destructive"}
                  `}
                >
                  <option value="include">Include</option>
                  <option value="exclude">Exclude</option>
                </select>

                <div className="flex-1" />

                <button
                  onClick={() => onRemoveFilter(filter.id)}
                  className="text-muted-foreground hover:text-destructive p-1.5 rounded-lg hover:bg-destructive/10 transition-colors"
                >
                  <Trash2 size={16} />
                </button>
              </div>

              <input
                type="text"
                value={filter.pattern}
                onChange={(e) => onUpdateFilter(filter.id, { pattern: e.target.value })}
                placeholder="Regex pattern..."
                className="w-full bg-accent/40 border border-border/50 rounded-xl px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary/40 focus:bg-accent/80 font-mono transition-all"
              />

              <div className="flex gap-1.5 overflow-x-auto pb-1 custom-scrollbar">
                {COLORS.map(color => (
                  <button
                    key={color}
                    onClick={() => onUpdateFilter(filter.id, { color })}
                    className={`
                      w-5 h-5 rounded-full border border-white/10 flex-shrink-0 transition-transform
                      hover:scale-125 active:scale-95
                      ${filter.color === color ? "ring-2 ring-primary ring-offset-2 ring-offset-background scale-110" : ""}
                    `}
                    style={{ backgroundColor: color }}
                  />
                ))}
                <button
                  onClick={() => onUpdateFilter(filter.id, { color: undefined })}
                  className={`
                    w-5 h-5 rounded-full border border-border/50 flex-shrink-0 bg-transparent flex items-center justify-center 
                    hover:scale-125 active:scale-95 transition-transform
                    ${!filter.color ? "ring-2 ring-primary ring-offset-2 ring-offset-background scale-110" : ""}
                  `}
                  title="No color"
                >
                  <X size={12} className="text-muted-foreground" />
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
      <path d="M18 6 6 18" /><path d="m6 6 12 12" />
    </svg>
  );
}
