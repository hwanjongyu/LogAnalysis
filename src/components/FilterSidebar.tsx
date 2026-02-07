import { Plus, Trash2, Eye, EyeOff, ChevronLeft, ChevronRight } from "lucide-react";

export interface LogFilter {
  id: string;
  pattern: string;
  is_include: boolean;
  is_enabled: boolean;
  color?: string;
  text_color?: string;
}

interface FilterSidebarProps {
  filters: LogFilter[];
  onAddFilter: () => void;
  onUpdateFilter: (id: string, updates: Partial<LogFilter>) => void;
  onRemoveFilter: (id: string) => void;
  onMoveFilter: (id: string, direction: "left" | "right") => void;
  filterCounts: Record<string, number>;
}


export function FilterSidebar({
  filters,
  onAddFilter,
  onUpdateFilter,
  onRemoveFilter,
  onMoveFilter,
  filterCounts
}: FilterSidebarProps) {
  return (
    <div className="flex-1 overflow-y-auto p-3 flex flex-wrap gap-2 items-start content-start custom-scrollbar">
      <div className="w-full flex justify-between items-center mb-1 px-1">
        <h2 className="font-bold text-[10px] uppercase tracking-widest text-muted-foreground/50">Filters</h2>
        <button
          onClick={onAddFilter}
          className="flex items-center gap-1.5 px-3 py-1 bg-primary/10 text-primary hover:bg-primary/20 rounded-lg transition-all text-[10px] font-bold uppercase tracking-wider"
        >
          <Plus size={14} />
          Add Filter
        </button>
      </div>

      {filters.length === 0 ? (
        <div className="h-24 flex items-center justify-center flex-1 text-xs text-muted-foreground/20 font-medium italic">
          No patterns defined.
        </div>
      ) : (
        filters.map((filter) => (
          <div
            key={filter.id}
            className={`
              p-2.5 rounded-xl border border-border/40 bg-card/40 space-y-2 w-80 shrink-0
              transition-all duration-base
              hover:shadow-md hover:border-primary/20 hover:bg-card
              shadow-sm relative
              ${!filter.is_enabled ? "opacity-30 grayscale" : ""}
            `}
          >
            <div className="flex items-center gap-1.5">
              <button
                onClick={() => onUpdateFilter(filter.id, { is_enabled: !filter.is_enabled })}
                className={`p-1 rounded-md transition-colors ${filter.is_enabled ? "text-primary hover:bg-primary/20" : "text-muted-foreground hover:bg-accent"}`}
              >
                {filter.is_enabled ? <Eye size={12} /> : <EyeOff size={12} />}
              </button>

              <div className="flex items-center gap-1 group/colors relative">
                <input
                  type="color"
                  value={filter.color || "#fa5feb"}
                  onChange={(e) => onUpdateFilter(filter.id, { color: e.target.value })}
                  className="w-4 h-4 rounded-full overflow-hidden border border-border/20 cursor-pointer bg-transparent"
                  title="Background color"
                />
                <input
                  type="color"
                  value={filter.text_color || "#121212"}
                  onChange={(e) => onUpdateFilter(filter.id, { text_color: e.target.value })}
                  className="w-4 h-4 rounded-md overflow-hidden border border-border/20 cursor-pointer bg-transparent"
                  title="Text color"
                />
              </div>

              <select
                value={filter.is_include ? "include" : "exclude"}
                onChange={(e) => onUpdateFilter(filter.id, { is_include: e.target.value === "include" })}
                className={`
                  bg-accent/40 border border-border/30 text-[9px] px-1.5 py-0.5 rounded-md uppercase font-bold tracking-tight
                  transition-all cursor-pointer focus:outline-none focus:ring-1 focus:ring-primary/20
                  ${filter.is_include ? "text-success" : "text-destructive"}
                `}
              >
                <option value="include">Inc</option>
                <option value="exclude">Exc</option>
              </select>

              <div className="flex items-center gap-1 px-1.5 py-0.5 bg-accent/10 rounded-md border border-border/5">
                <span className="text-[10px] font-mono font-bold text-foreground/70 tabular-nums">
                  {(filterCounts[filter.id] || 0).toLocaleString()}
                </span>
              </div>

              <div className="flex flex-1 justify-end items-center gap-0.5">
                <button
                  onClick={() => onMoveFilter(filter.id, "left")}
                  className="text-muted-foreground/30 hover:text-primary p-0.5 disabled:opacity-0"
                  disabled={filters.indexOf(filter) === 0}
                >
                  <ChevronLeft size={10} />
                </button>
                <button
                  onClick={() => onMoveFilter(filter.id, "right")}
                  className="text-muted-foreground/40 hover:text-primary p-0.5 disabled:opacity-0"
                  disabled={filters.indexOf(filter) === filters.length - 1}
                >
                  <ChevronRight size={10} />
                </button>
                <button
                  onClick={() => onRemoveFilter(filter.id)}
                  className="text-muted-foreground/30 hover:text-destructive p-0.5 rounded-md hover:bg-destructive/10 transition-colors"
                >
                  <Trash2 size={12} />
                </button>
              </div>
            </div>

            <input
              type="text"
              value={filter.pattern}
              onChange={(e) => onUpdateFilter(filter.id, { pattern: e.target.value })}
              placeholder="Pattern..."
              className="w-full bg-accent/20 border border-border/20 rounded-lg px-2 py-1 text-[11px] focus:outline-none focus:ring-1 focus:ring-primary/20 focus:bg-accent/40 font-mono transition-all"
            />
          </div>
        ))
      )}
    </div>
  );
}

