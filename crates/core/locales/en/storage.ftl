## Errors of the `storage` module (the data folder).

storage-io = Could not read or write { $path }: { $reason }
storage-json = The file { $path } is damaged or incomplete: { $reason }
storage-too-new = The file { $path } was written by a newer Exodus (format { $found }, this version knows up to { $supported }). Update Exodus to use it; the file was left unchanged.
storage-layout-too-new = The data folder was set up by a newer Exodus (layout { $found }, this version knows up to { $supported }). This version can only read it. Update Exodus to make changes.
storage-busy = Another Exodus window is busy changing the data folder. Try again in a moment.
storage-migration = Could not update the old file { $path } to the current format: { $reason }
