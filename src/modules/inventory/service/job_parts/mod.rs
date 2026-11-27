//! The stock parts a job uses: finding each in stock, pricing and costing it, and taking it out.
//! Repair jobs and print jobs share it. What a job charges for a part is the job's own rule.

mod planned_part;

pub(crate) use planned_part::JobPartPick;
pub(crate) use planned_part::PlannedPart;
pub(crate) use planned_part::plan_job_parts;
pub(crate) use planned_part::take_job_parts;
