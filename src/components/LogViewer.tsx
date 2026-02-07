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

  const getLineStyle = (line: string) => {
    const activeFilters = filters.filter(f => f.is_enabled && f.color);
    for (const filter of activeFilters) {
      try {
        const re = new RegExp(filter.pattern);
        if (re.test(line)) {
          return { backgroundColor: `${filter.color}20`, borderLeftColor: filter.color };
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
          className="px-4 py-0.5 text-muted-foreground/30 border-l-2 border-transparent"
          style={{ fontSize: `${fontSize}px`, minHeight: `${fontSize * 1.5}px` }}
        >
          ...
        </div>
      );
    }

    const style = getLineStyle(line);

    return (
      <div 
        className="px-4 py-0.5 hover:bg-accent/30 border-l-2 border-transparent whitespace-pre font-mono"
        style={{ fontSize: `${fontSize}px`, ...style }}
      >
        <span className="inline-block w-12 text-muted-foreground/40 select-none mr-4 text-right tabular-nums">
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