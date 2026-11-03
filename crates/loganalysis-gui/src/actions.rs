use gpui::actions;

actions!(
    loganalysis,
    [
        OpenFile,
        ToggleFilteredOnly,
        Find,
        FindNext,
        FindPrev,
        CloseFind,
        StartAdb,
        StopAdb,
        RefreshAdbDevices,
        AddFilter,
    ]
);
