import { useEffect, useRef, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { LogFilter } from "./FilterSidebar";

interface MinimapProps {
    filePath: string | null;
    filters: LogFilter[];
    searchQuery: string;
    totalLines: number;
    onScrollTo: (index: number) => void;
    visibleRange: { startIndex: number; endIndex: number };
}

interface MinimapBucket {
    intensity: number;
    color: string | null;
}

export function Minimap({ filePath, filters, searchQuery, totalLines, onScrollTo, visibleRange }: MinimapProps) {
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const [minimapData, setMinimapData] = useState<MinimapBucket[]>([]);
    const containerRef = useRef<HTMLDivElement>(null);
    const [isReady, setIsReady] = useState(false);
    const [hoverY, setHoverY] = useState<number | null>(null);

    useEffect(() => {
        // Small delay to ensure container is mounted and has height
        const timer = setTimeout(() => setIsReady(true), 100);
        return () => clearTimeout(timer);
    }, []);

    const fetchMinimapData = useCallback(async () => {
        if (!filePath || totalLines === 0 || !isReady) return;

        try {
            const height = containerRef.current?.clientHeight || 800;
            const data = await invoke<MinimapBucket[]>("get_minimap_data", {
                filters,
                searchQuery,
                buckets: height
            });
            setMinimapData(data);
        } catch (e) {
            console.error("Failed to fetch minimap data", e);
        }
    }, [filePath, filters, searchQuery, totalLines, isReady]);

    useEffect(() => {
        fetchMinimapData();
    }, [fetchMinimapData]);

    useEffect(() => {
        const canvas = canvasRef.current;
        if (!canvas || minimapData.length === 0) return;

        const ctx = canvas.getContext('2d');
        if (!ctx) return;

        const { width, height } = canvas;
        ctx.clearRect(0, 0, width, height);

        // Draw matches
        minimapData.forEach((bucket, i) => {
            if (bucket.intensity > 0 && bucket.color) {
                ctx.fillStyle = bucket.color;
                const alpha = Math.min(0.3 + bucket.intensity * 0.7, 1.0);
                ctx.globalAlpha = alpha;
                ctx.fillRect(0, i, width, 1);
            }
        });

        // Draw visible range highlight
        if (totalLines > 0) {
            const startY = (visibleRange.startIndex / totalLines) * height;
            const endY = (visibleRange.endIndex / totalLines) * height;

            ctx.globalAlpha = 1.0;
            ctx.fillStyle = 'rgba(255, 255, 255, 0.05)';
            ctx.fillRect(0, startY, width, Math.max(2, endY - startY));

            ctx.strokeStyle = 'rgba(255, 255, 255, 0.2)';
            ctx.lineWidth = 1;
            ctx.strokeRect(0.5, startY, width - 1, Math.max(2, endY - startY));
        }

    }, [minimapData, visibleRange, totalLines]);

    const handleInteraction = (e: React.MouseEvent | React.TouchEvent) => {
        const canvas = canvasRef.current;
        if (!canvas || totalLines === 0) return;
        const rect = canvas.getBoundingClientRect();

        let clientY: number;
        if ('touches' in e) {
            clientY = e.touches[0].clientY;
        } else {
            clientY = e.clientY;
        }

        const y = Math.max(0, Math.min(clientY - rect.top, rect.height));
        const index = Math.floor((y / rect.height) * totalLines);
        onScrollTo(index);
    };

    return (
        <div
            ref={containerRef}
            className="w-12 h-full border-l border-white/5 relative bg-black/10 backdrop-blur-sm overflow-hidden cursor-pointer select-none group"
            onMouseMove={(e) => {
                const rect = containerRef.current?.getBoundingClientRect();
                if (rect) setHoverY(e.clientY - rect.top);
            }}
            onMouseLeave={() => setHoverY(null)}
            onMouseDown={(e) => {
                handleInteraction(e);
                const handleMouseMove = (me: MouseEvent) => {
                    handleInteraction(me as any);
                    const rect = containerRef.current?.getBoundingClientRect();
                    if (rect) setHoverY(me.clientY - rect.top);
                };
                const handleMouseUp = () => {
                    window.removeEventListener('mousemove', handleMouseMove);
                    window.removeEventListener('mouseup', handleMouseUp);
                };
                window.addEventListener('mousemove', handleMouseMove);
                window.addEventListener('mouseup', handleMouseUp);
            }}
        >
            <canvas
                ref={canvasRef}
                width={48}
                height={minimapData.length || 800}
                className="w-full h-full"
            />

            {hoverY !== null && (
                <div
                    className="absolute left-0 right-0 h-[1px] bg-primary/40 pointer-events-none z-10 shadow-[0_0_8px_rgba(250,95,235,0.6)]"
                    style={{ top: hoverY }}
                />
            )}

            {/* Pro Glow Edge */}
            <div className="absolute inset-y-0 left-0 w-[1px] bg-gradient-to-b from-transparent via-primary/30 to-transparent" />
        </div>
    );
}
