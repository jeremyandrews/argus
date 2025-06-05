# Analysis Worker Bug Investigation - June 5, 2025

## Issue Summary

**Critical Bug**: Analysis workers stopped functioning and are exiting cleanly after recent cluster matching logic changes. Workers stop processing analysis queues despite having 83 items waiting.

## Timeline

- **Issue Started**: After cluster matching logic fix in previous commit
- **Failure Time**: 2025-06-05T07:09:03 UTC (last successful analysis worker activity)
- **Discovery Time**: 2025-06-05T07:36:14 CEST (09:36 UTC) - 2.5 hours after failure
- **Current Status**: Analysis workers not running, 83 items waiting in analysis queues

## Initial Diagnosis: Database Schema Issues

### Original Error (Resolved)
- **Root Cause**: Missing `title` column in `articles` table
- **Error**: "no such column: a.title" in cluster summary generation
- **Evidence**: Log entry at 07:09:04.012810 showing SQL error

### Schema Fixes Applied
Successfully added missing columns to production database:

#### `articles` table fixes:
```sql
ALTER TABLE articles ADD COLUMN title TEXT;
ALTER TABLE articles ADD COLUMN json_data TEXT;
ALTER TABLE articles ADD COLUMN quality REAL;
ALTER TABLE articles ADD COLUMN source TEXT;
```

#### Schema verification:
- ✅ All required columns now present in articles table
- ✅ article_clusters table has all expected columns (status, name, etc.)
- ✅ cluster_merge_history table exists
- ✅ Database queries work correctly

## Current Issue: Worker Logic Problem

### Evidence Analysis Workers Stopped
- **Last Activity**: 2025-06-05T08:04:19 UTC (analysis worker 2 completed successfully)
- **No Error Logs**: No ERROR/WARN entries around failure time
- **Clean Exit**: Workers appear to exit gracefully, not crash
- **Queue Status**: 40 items in life_safety_queue, 43 in matched_topics_queue
- **Decision Workers**: Continue functioning normally

### Investigation Results

#### What We Ruled Out:
1. **Database Schema Issues**: ✅ Resolved - all columns present
2. **Connectivity Issues**: ✅ Ollama servers responding correctly
3. **Worker Crashes**: ✅ No panic/crash logs found
4. **Queue Access Issues**: ✅ Queues accessible and contain items
5. **Fallback Mode Logic**: ✅ Workers appear to exit rather than switch modes

#### What We Confirmed:
1. **Workers Are Configured**: Logs show `process_analysis_item{worker_id=0}` entries
2. **Schema Is Correct**: Database matches code expectations
3. **Queues Have Work**: 83 items waiting for processing
4. **Application Is Running**: Main process and decision workers active

## Current Hypothesis

**The 'PRIMARY' → 'Primary' case fix was correct and enabled a previously broken clustering code path that has a bug.**

**Root Cause Analysis:**
- **BEFORE**: Query used `importance = 'PRIMARY'` but database stores `'Primary'`
- **RESULT**: Case mismatch caused `get_article_entities()` to always return empty results
- **IMPACT**: Empty results meant clustering logic was never executed (workers didn't crash)
- **FIX APPLIED**: Changed query to `importance = 'Primary'` (correct case)
- **NEW RESULT**: Query now returns actual entity IDs from articles
- **CURRENT PROBLEM**: Clustering logic that follows successful entity retrieval has a bug causing worker exits

**The clustering code path is now active for the first time and contains a bug that causes analysis workers to exit cleanly.**

## Diagnostic Commands Used

### Schema Verification:
```sql
.schema articles
.schema article_clusters
.tables
```

### Log Analysis:
```bash
grep "analysis\ worker" logs/app.log.2025-06-05 | tail -5
sed -n '/2025-06-05T08:04:19/,/2025-06-05T08:05/p' logs/app.log.2025-06-05
grep "^[0-9].*ERROR\|^[0-9].*WARN" logs/app.log.2025-06-05
```

### Queue Status:
```sql
SELECT COUNT(*) FROM life_safety_queue;
SELECT COUNT(*) FROM matched_topics_queue;
```

## Files Modified During Investigation

1. **src/db/schema.rs**: Updated to match production database structure
   - Added missing columns to articles table definition
   - Fixed table name mismatch (article_cluster_mappings)
   - Added complete article_clusters schema
   - Added cluster_merge_history table

## SOLUTION IMPLEMENTED ✅

**Root Cause Identified and Fixed:**

The analysis workers were **panicking** due to an `unimplemented!()` macro in the clustering code.

**Technical Details:**
1. **Before Fix**: Case mismatch (`'PRIMARY'` vs `'Primary'`) caused `get_article_entities()` to return empty results
2. **Result**: `assign_article_to_cluster()` returned `Ok(0)`, clustering logic never executed
3. **After Fix**: Query now returns actual entity IDs, clustering logic executes for first time
4. **New Problem**: Workers call `check_and_merge_similar_clusters()` which contains `unimplemented!()`
5. **Panic**: `unimplemented!()` macro panics and exits the worker

**Files Fixed:**
- **src/clustering/merging/similarity.rs**: Replaced `unimplemented!()` with safe stubs
  - `check_and_merge_similar_clusters()` now returns `Ok(None)`
  - `find_clusters_with_entity_overlap()` now returns `Ok(Vec::new())`

**Impact:**
- ✅ Analysis workers will no longer panic
- ✅ Clustering logic will function without crashing
- ✅ Workers should resume processing the 83 queued items
- ⚠️ Cluster merging functionality temporarily disabled (returns no-op)

## Next Steps

1. **Deploy Fix**: Restart argus application to load the fixed code
2. **Monitor Workers**: Watch for analysis worker resumption and processing
3. **Implement Full Logic**: Complete the cluster merging implementation later
4. **Verify Recovery**: Confirm 83 queued items are processed

## Technical Details

### Database Schema Status
- **articles table**: ✅ Complete (title, json_data, quality, source added)
- **article_clusters table**: ✅ Complete (all columns present)
- **cluster_merge_history table**: ✅ Present
- **Queue tables**: ✅ Accessible and populated

### Worker Architecture
- **Threading Model**: Workers run as threads, not separate processes
- **Fallback Logic**: Analysis workers can switch to decision mode when idle
- **Queue Processing**: Workers should process life_safety_queue and matched_topics_queue

### Key Log Patterns
- Last successful analysis: `analysis worker 2 qwen3:30b-a3b-fp16 http://10.20.232.78:11434]: successfully generated response`
- No switching logs: No "switching to Decision Worker" or "switching back" messages
- Clean continuation: Decision workers continue processing normally after analysis workers stop

## Lessons Learned

1. **Database schema mismatches can cause silent failures** in clustering operations
2. **Worker exit conditions may not be properly logged** in current implementation
3. **Schema validation is critical** after code changes that add database operations
4. **Comprehensive schema migration commands are essential** to avoid partial fixes
