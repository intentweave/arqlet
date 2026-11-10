// Request provenance is independent of display animation and graph contents.
export function refinementFits(current, result, base, epoch, editPending) {
  return !editPending && epoch===base.epoch &&
    current.revision===base.revision && (current.layoutVersion??0)===base.layoutVersion &&
    result.revision===base.revision && result.baseRevision===base.revision &&
    result.layoutVersion===base.layoutVersion+1;
}

// A progressive publication may be shown only while its job is still the one the user waits for: the same job, the
// next sequence number, and the job's base still current. The base is the state the job started from; the displayed
// snapshot moves forward with every shown publication, so it is never compared here.
export function acceptPublication(publication, job, epoch, editPending) {
  return publication.jobId===job.id && publication.sequence>job.lastSequence &&
    refinementFits({revision: job.revision, layoutVersion: job.layoutVersion}, publication.snapshot, job, epoch, editPending);
}
