import { useState, useEffect, useRef, useCallback } from "react";
import { Virtuoso, VirtuosoHandle } from "react-virtuoso";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface LogViewerProps {
  filePath: string | null;
  lineCount: number;
  fontSize: number;
}

export function LogViewer({ filePath, lineCount, fontSize }: LogViewerProps) {
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const [loadedLines, setLoadedLines] = useState<Map<number, string>>(new Map());

  // Clear cache when file changes
  useEffect(() => {
    setLoadedLines(new Map());
  }, [filePath]);

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

  const rowRenderer = (index: number) => {
    const line = loadedLines.get(index);
    
    // If line isn't loaded, trigger a batch load
    if (line === undefined) {
      // Load a buffer of 100 lines around the current index
      const batchStart = Math.max(0, index - 50);
      loadMore(batchStart, 100);
      return (
        <div 
          className="px-4 py-0.5 text-muted-foreground/50 border-l-2 border-transparent animate-pulse"
          style={{ fontSize: `${fontSize}px`, minHeight: `${fontSize * 1.5}px` }}
        >
          Loading...
        </div>
      );
    }

    return (
      <div 
        className="px-4 py-0.5 hover:bg-accent/30 border-l-2 border-transparent hover:border-primary/50 whitespace-pre font-mono"
        style={{ fontSize: `${fontSize}px` }}
      >
        <span className="inline-block w-12 text-muted-foreground/50 select-none mr-4 text-right">
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
        increaseViewportBy={200}
        className="h-full scrollbar-thin scrollbar-thumb-muted-foreground/20"
      />
    </div>
  );
}
