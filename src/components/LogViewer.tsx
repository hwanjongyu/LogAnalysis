import { useState, useEffect, useRef, useCallback } from "react";
import { Virtuoso, VirtuosoHandle } from "react-virtuoso";
import { invoke } from "@tauri-apps/api/core";
import { LogFilter } from "./FilterSidebar";
import { Minimap } from "./Minimap";

interface LogViewerProps {
  filePath: string | null;
  lineCount: number;
  fontSize: number;
  filters: LogFilter[];
  searchQuery: string;
  isHighDensity?: boolean;
}

export function LogViewer({ filePath, lineCount, fontSize, filters, searchQuery, isHighDensity }: LogViewerProps) {
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const [loadedLines, setLoadedLines] = useState<Map<number, string>>(new Map());
  const [visibleRange, setVisibleRange] = useState({ startIndex: 0, endIndex: 100 });

  // Clear cache when file OR filters OR search change
  useEffect(() => {
    setLoadedLines(new Map());
  }, [filePath, lineCount, searchQuery]);

  const loadMore = useCallback(async (startIndex: number, count: number) => {
    try {
      const lines = await invoke<string[]>("get_log_lines", {
        startIndex,
        count: Math.min(count, lineCount - startIndex),
      });

      setLoadedLines((prev) => {
        const next = new Map(prev);
        lines.forEach((line, i) => {
          next.set(startIndex + i, line);
        });
        return next;
      });
    } catch (error) {
      console.error("Failed to fetch log lines:", error);
    }
  }, [lineCount]);

  const getLogLevel = (line: string): 'ERROR' | 'WARN' | 'INFO' | 'DEBUG' | 'VERBOSE' | null => {
    // Standard Android/Unix patterns: [E],  E , ERROR, E/
    if (/\sE\s|\[E\]|ERROR|E\//i.test(line)) return 'ERROR';
    if (/\sW\s|\[W\]|WARN|W\//i.test(line)) return 'WARN';
    if (/\sI\s|\[I\]|INFO|I\//i.test(line)) return 'INFO';
    if (/\sD\s|\[D\]|DEBUG|D\//i.test(line)) return 'DEBUG';
    if (/\sV\s|\[V\]|VERBOSE|V\//i.test(line)) return 'VERBOSE';
    return null;
  };

  const getLevelStyle = (level: string | null) => {
    switch (level) {
      case 'ERROR':
        return {
          backgroundColor: 'rgba(239, 68, 68, 0.12)',
          borderLeftColor: '#ef4444',
        };
      case 'WARN':
        return {
          backgroundColor: 'rgba(245, 158, 11, 0.12)',
          borderLeftColor: '#f59e0b',
        };
      case 'INFO':
        return {
          backgroundColor: 'rgba(59, 130, 246, 0.08)',
          borderLeftColor: '#3b82f6',
        };
      case 'DEBUG':
        return {
          backgroundColor: 'rgba(100, 116, 139, 0.05)',
          borderLeftColor: '#64748b',
        };
      case 'VERBOSE':
        return {
          backgroundColor: 'rgba(148, 163, 184, 0.03)',
          borderLeftColor: '#94a3b8',
        };
      default:
        return {};
    }
  };

  const getLineStyle = (line: string) => {
    const activeFilters = filters.filter(f => f.is_enabled && f.is_include);
    for (const filter of activeFilters) {
      if (!filter.pattern) continue;
      try {
        const re = new RegExp(filter.pattern, 'i');
        if (re.test(line)) {
          return {
            backgroundColor: filter.color ? `${filter.color}33` : "rgba(250, 95, 235, 0.2)",
            borderLeftColor: filter.color || "#fa5feb",
            color: filter.text_color || undefined,
          };
        }
      } catch (e) {
        // Skip invalid regex
      }
    }
    return {};
  };

  const renderLineContent = (line: string) => {
    if (!searchQuery || searchQuery.length < 2) return line;

    try {
      // Escape special regex characters
      const escapedQuery = searchQuery.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      const parts = line.split(new RegExp(`(${escapedQuery})`, "gi"));
      return parts.map((part, i) =>
        part.toLowerCase() === searchQuery.toLowerCase() ? (
          <mark key={i} className="bg-primary/40 text-white rounded-sm px-0.5 border-b border-primary shadow-[0_0_8px_rgba(250,95,235,0.4)]">
            {part}
          </mark>
        ) : (
          part
        )
      );
    } catch (e) {
      return line;
    }
  };

  const rowRenderer = (index: number) => {
    const line = loadedLines.get(index);

    if (line === undefined) {
      const batchStart = Math.max(0, index - 50);
      loadMore(batchStart, 100);
      return (
        <div
          className="px-4 py-0.5 text-muted-foreground/20 border-l-2 border-transparent animate-pulse"
          style={{ fontSize: `${fontSize}px`, minHeight: `${fontSize * 1.5}px` }}
        >
          ● ● ●
        </div>
      );
    }

    const level = getLogLevel(line);
    const levelStyle = getLevelStyle(level);
    const filterStyle = getLineStyle(line);
    const combinedStyle = { ...levelStyle, ...filterStyle };
    const isEven = index % 2 === 0;

    return (
      <div
        className={`
          px-4 border-l-2 border-transparent whitespace-pre font-mono text-foreground
          transition-all duration-fast
          hover:bg-primary/5 hover:border-l-primary/50
          ${isEven ? 'bg-black/[0.02]' : 'bg-transparent'}
          ${isHighDensity ? 'py-0 tracking-tight' : 'py-0.5'}
        `}
        style={{
          fontSize: `${isHighDensity ? fontSize - 1 : fontSize}px`,
          lineHeight: `${fontSize * (isHighDensity ? 1.2 : 1.5)}px`,
          ...combinedStyle
        }}
      >
        <span className="inline-block w-12 text-muted-foreground/30 select-none mr-4 text-right tabular-nums font-normal border-r border-border/20 pr-4">
          {index + 1}
        </span>
        {renderLineContent(line || " ")}
      </div>
    );
  };

  if (!filePath) {
    return null;
  }

  return (
    <div className="w-full h-full bg-background overflow-hidden flex">
      <div className="flex-1 min-w-0">
        <Virtuoso
          ref={virtuosoRef}
          totalCount={lineCount}
          itemContent={rowRenderer}
          initialTopMostItemIndex={0}
          increaseViewportBy={300}
          className="h-full scrollbar-thin scrollbar-thumb-muted-foreground/20"
          rangeChanged={setVisibleRange}
        />
      </div>
      {(filePath || lineCount > 0) && (
        <Minimap
          filePath={filePath}
          filters={filters}
          searchQuery={searchQuery}
          totalLines={lineCount}
          onScrollTo={(index) => virtuosoRef.current?.scrollToIndex({ index, align: 'start', behavior: 'auto' })}
          visibleRange={visibleRange}
        />
      )}
    </div>
  );
}