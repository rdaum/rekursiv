use super::*;
#[derive(Clone, Copy)]
pub(super) struct PendingStore {
    request: StoreRequest,
    left: u32,
    fail: bool,
}
impl Harness<'_> {
    fn store_request(&self) -> Result<StoreRequest> {
        Ok(StoreRequest {
            op: self.rtl.store_op_o.try_into()?,
            reference: Word::from_bits(self.rtl.store_ref_o)?,
            class: Word::from_bits(self.rtl.store_class_o)?,
            size: self.rtl.store_size_o,
            cond: self.rtl.store_cond_o != 0,
            offset: self.rtl.store_offset_o,
            data: Word::from_bits(self.rtl.store_data_o)?,
        })
    }
    pub(super) fn tick_store(&mut self) -> Result<()> {
        if let Some(request) = self.held_store {
            ensure!(
                self.rtl.store_valid_o != 0 && self.store_request()? == request,
                "backing-store request changed under backpressure"
            );
        }
        self.rtl.store_ready_i = (self.pending_store.is_none()
            && self.store_wait >= self.store_timing.request_delay)
            as u8;
        self.rtl.store_rsp_valid_i = 0;
        let mut prepared = None;
        if let Some(p) = self.pending_store {
            if p.left == 0 {
                let mut next = self.store.clone();
                let reply = next.request(p.request, p.fail);
                self.rtl.store_rsp_valid_i = 1;
                self.rtl.store_rsp_status_i = reply.status as u8;
                self.rtl.store_rsp_ref_i = reply.reference.bits();
                self.rtl.store_rsp_class_i = reply.class.bits();
                self.rtl.store_rsp_size_i = reply.size;
                self.rtl.store_rsp_cond_i = reply.cond as u8;
                self.rtl.store_rsp_data_i = reply.data.bits();
                prepared = Some(next);
            }
        }
        self.rtl.eval();
        let request = self.rtl.store_valid_o != 0 && self.rtl.store_ready_i != 0;
        let complete = self.rtl.store_rsp_valid_i != 0 && self.rtl.store_rsp_ready_o != 0;
        self.held_store = if self.rtl.store_valid_o != 0 && !request {
            Some(self.store_request()?)
        } else {
            None
        };
        if complete {
            let pending = self.pending_store.take().unwrap();
            self.store = prepared.unwrap();
            if self.rtl.store_rsp_status_i == 0 && pending.request.op == StoreOp::CommitSave {
                self.stats.saved_objects += 1;
            }
        } else if let Some(p) = self.pending_store.as_mut() {
            p.left = p.left.saturating_sub(1);
        }
        if request {
            ensure!(
                self.pending_store.is_none(),
                "second outstanding backing-store transaction"
            );
            let req = self.store_request()?;
            self.pending_store = Some(PendingStore {
                request: req,
                left: self.store_timing.memory_latency,
                fail: self.store_fault == Some(self.store_requests.len()),
            });
            self.store_requests.push(req);
            self.stats.store_transactions += 1;
            self.store_wait = 0;
        } else if self.rtl.store_valid_o != 0 {
            self.store_wait += 1;
        } else {
            self.store_wait = 0;
        }
        Ok(())
    }
}
