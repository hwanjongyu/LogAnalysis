import { useState, useEffect, useRef, useCallback } from "react";
import { Virtuoso, VirtuosoHandle } from "react-virtuoso";
import { invoke } from "@tauri-apps/api/core";
import { LogFilter } from "./FilterSidebar";

interface LogViewerProps {
  filePath: string | null;
  lineCount: number;
  fontSize: number;
  filters: LogFilter[];
}

export function LogViewer({ filePath, lineCount, fontSize, filters }: LogViewerProps) {
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const [loadedLines, setLoadedLines] = useState<Map<number, string>>(new Map());

  // Clear cache when file OR filters change
  useEffect(() => {
    setLoadedLines(new Map());
  }, [filePath, lineCount]); // lineCount changes when filters are applied

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

  const getLogLevel = (line: string): 'ERROR' | 'WARN' | 'INFO' | 'DEBUG' | null => {
    const upperLine = line.toUpperCase();
    if (upperLine.includes('[ERROR]') || upperLine.includes(' ERROR ')) return 'ERROR';
    if (upperLine.includes('[WARN]') || upperLine.includes(' WARN ') || upperLine.includes('[WARNING]')) return 'WARN';
    if (upperLine.includes('[INFO]') || upperLine.includes(' INFO ')) return 'INFO';
    if (upperLine.includes('[DEBUG]') || upperLine.includes(' DEBUG ')) return 'DEBUG';
    return null;
  };

  const getLevelStyle = (level: string | null) => {
    switch (level) {
      case 'ERROR':
        return {
          backgroundColor: 'rgba(239, 68, 68, 0.12)',
          borderLeftColor: '#ef4444',
          color: '#fca5a5'
        };
      case 'WARN':
        return {
          backgroundColor: 'rgba(245, 158, 11, 0.12)',
          borderLeftColor: '#f59e0b',
          color: '#fcd34d'
        };
      case 'INFO':
        return {
          backgroundColor: 'rgba(59, 130, 246, 0.08)',
          borderLeftColor: '#3b82f6',
          color: '#93c5fd'
        };
      case 'DEBUG':
        return {
          backgroundColor: 'rgba(100, 116, 139, 0.06)',
          borderLeftColor: '#64748b',
          color: '#94a3b8'
        };
      default:
        return {};
    }
  };

  const getLineStyle = (line: string) => {
    const activeFilters = filters.filter(f => f.is_enabled && f.color);
    for (const filter of activeFilters) {
      try {
        const re = new RegExp(filter.pattern, 'i');
        if (re.test(line)) {
          return { backgroundColor: `${filter.color}25`, borderLeftColor: filter.color };
        }
      } catch (e) {
        // Skip invalid regex
      }
    }
    return {};
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
          px-4 py-0.5 border-l-2 border-transparent whitespace-pre font-mono
          transition-all duration-fast
          hover:bg-primary/5 hover:border-l-primary/50
          ${isEven ? 'bg-white/[0.02]' : 'bg-transparent'}
        `}
        style={{
          fontSize: `${fontSize}px`,
          lineHeight: `${fontSize * 1.5}px`,
          ...combinedStyle
        }}
      >
        <span className="inline-block w-12 text-muted-foreground/30 select-none mr-4 text-right tabular-nums font-normal border-r border-border/20 pr-4">
          {index + 1}
        </span>
        {line || " "}
      </div>
    );
  };

  if (!filePath) {
    return null;
  }

  return (
    <div className="w-full h-full bg-background overflow-hidden">
      <Virtuoso
        ref={virtuosoRef}
        totalCount={lineCount}
        itemContent={rowRenderer}
        initialTopMostItemIndex={0}
        increaseViewportBy={300}
        className="h-full scrollbar-thin scrollbar-thumb-muted-foreground/20"
      />
    </div>
  );
}