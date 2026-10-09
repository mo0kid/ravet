"""Explain Rust expression mechanics alongside the reviewed domain-specific chapters."""
import re

OPERATIONS = [
 ('.into_iter()', 'Consume the collection into an owning iterator; its elements are moved into the following transformation.'),
 ('.iter_mut()', 'Borrow mutable collection entries one at a time so the following code can update existing storage.'),
 ('.iter()', 'Borrow collection entries for iteration without moving or copying the whole collection.'),
 ('.collect::<Vec', 'Collect the iterator into an owned Vec. This may allocate and belongs to setup, offline fixtures or the main-thread UI path.'),
 ('.zip(', 'Pair each entry with the corresponding entry in the second sequence. Iteration ends when either sequence ends.'),
 ('.any(', 'Return true if any entry satisfies this predicate; stop at the first match.'),
 ('.filter(', 'Keep only entries for which this predicate returns true.'),
 ('.map(', 'Transform each iterator entry, or the successful value of an Option/Result, using the supplied closure.'),
 ('.map_err(', 'Convert the error into the caller’s expected error type while leaving a successful result unchanged.'),
 ('.as_u64()', 'Accept a JSON unsigned integer. Missing, negative or incompatible values produce None for the subsequent validation.'),
 ('.as_array()', 'Require a JSON array and borrow its entries; incompatible JSON produces None.'),
 ('.as_str()', 'Require a JSON string and borrow its text; incompatible JSON produces None.'),
 ('.as_f64()', 'Read a JSON numeric value as f64; separate bounds/finite checks still determine whether the control is acceptable.'),
 ('.as_mut()', 'Borrow the contained value mutably without taking ownership; an absent Option stays absent.'),
 ('.as_ref()', 'Borrow the contained value without taking ownership; an absent Option stays absent.'),
 ('.push(', 'Append the supplied item to this owned collection. UI/fixture collections grow outside the audio callback.'),
 ('.extend(', 'Append the supplied sequence to this owned control/fixture collection.'),
 ('.clone()', 'Clone this value. Fixed arrays copy their entries; owned strings/vectors may allocate and remain outside the callback.'),
 ('.take()', 'Move the value out of this Option and leave None, so later cleanup cannot retain the same owner.'),
 ('.min(', 'Cap this quantity at the supplied upper limit.'),
 ('.max(', 'Keep this quantity at least as large as the supplied lower limit.'),
 ('.clamp(', 'Constrain the value between both supplied bounds.'),
 ('.round()', 'Round to the nearest integer-valued result before the following conversion or indexing operation.'),
 ('.ceil()', 'Round upward so storage/count calculations include the required fractional remainder.'),
 ('.sort_unstable()', 'Sort the candidate values in place; ordering of equal entries is irrelevant here.'),
 ('.dedup()', 'Remove adjacent duplicates after sorting so each candidate appears once.'),
 ('.retain(', 'Filter this existing Vec in place by the supplied supported-value predicate.'),
 ('.store(', 'Publish the new atomic flag/value using the specified memory ordering.'),
 ('.fetch_add(', 'Atomically increment this counter and retain the requested synchronization ordering.'),
 ('.pop()', 'Attempt to remove the next item from the bounded queue; an empty queue is handled by the surrounding branch.'),
 ('.play()', 'Request native playback after preparation. Failure is returned to the main-thread owner for cleanup.'),
 ('.pause()', 'Request native playback to stop before releasing or replacing its owner.'),
 ('.statistics()', 'Read the adapter’s current counters; fault/loss handling occurs in the surrounding code.'),
]


def statement_note(line, chapter):
    text=line.strip()
    if text.startswith(('}',')',']')) and re.fullmatch(r'[}\])\s,;]+',text):
        return 'Close the current expression or scope; no additional processing operation occurs on this line.'
    if text.startswith(('pub const ', 'const ')):
        return 'Declare this compile-time constant or fixed array. The value and physical/capacity meaning are given by its source comment and this chapter.'
    if text.startswith('type '):
        return 'Define a type alias so the following functions can use this complete result/error type by its shorter name.'
    if text.startswith(') ->'):
        return 'Complete the function’s parameter list and specify the return type. A Result preserves the explicit success/error distinction.'
    if text.startswith(('json!(', 'serde_json::json!(')):
        return 'Build the main-thread presentation JSON from these named values; the audio owner publishes fixed words instead of serializing JSON.'
    if text.startswith(('Ok(', 'Err(')):
        return 'Construct the success/error variant selected by the surrounding match or function return. The enclosed value becomes its payload.'
    if '=>' in text:
        return 'Handle the matching pattern on the left using the expression or scope on the right. Other variants are handled by the following arms.'
    for marker,note in OPERATIONS:
        if marker in text: return note
    field=re.fullmatch(r'(?:pub )?([\w]+):\s*(.+),?',text)
    if field:
        return 'Define or initialize '+field[1]+' with the displayed type/value. Its ownership and use are described in “'+chapter['title']+'”.'
    assignment=re.match(r'([\w.\[\] ()]+)\s*(\+=|\|=|-=|=)\s*(.+)',text)
    if assignment:
        operations={'=':'Assign the computed value to','+=':'Add the computed value to','-=':'Subtract the computed value from','|=':'Set the selected bits in'}
        return operations[assignment[2]]+' '+assignment[1].strip()+'. The right-hand expression supplies the new value for this step.'
    if text.startswith('&&'): return 'Require this additional condition as well; the combined check succeeds only when all conditions are true.'
    if text.startswith('||'): return 'Accept this alternative condition in the surrounding check; evaluation stops once the combined condition is true.'
    if text.startswith('else'): return 'Enter the alternative path when the preceding condition did not match.'
    if text.startswith('drop('): return 'Release this owner on the control/offline thread after its running activity has been stopped.'
    if re.fullmatch(r'[\w,: ]+,',text) and '::' not in text:
        return 'Supply these names as the remaining imports, arguments or fields of the surrounding declaration.'
    if re.fullmatch(r'[-\d_ .]+,?',text):
        return 'Supply this literal numeric entry to the array or argument list being constructed. Its units/addressing follow the containing fixture or configuration.'
    if re.search(r'\w+\([^;]*\);?$',text):
        return 'Call the displayed operation with these arguments as the next step in “'+chapter['title']+'”. The chapter describes its role and validation.'
    return 'Continue the surrounding value or expression with this exact operand. Read it together with the preceding statement and the “'+chapter['title']+'” explanation above.'
