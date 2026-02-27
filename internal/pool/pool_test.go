package pool

import (
	"errors"
	"sync/atomic"
	"testing"
)

func TestPool(t *testing.T) {
	var count int32
	p := NewPool(5)
	
	for i := 0; i < 10; i++ {
		p.Submit(func() error {
			atomic.AddInt32(&count, 1)
			return nil
		})
	}
	
	p.Wait()
	
	if count != 10 {
		t.Errorf("Expected count 10, got %d", count)
	}
}

func TestPoolError(t *testing.T) {
	p := NewPool(2)
	
	p.Submit(func() error {
		return errors.New("test error")
	})
	
	err := p.Wait()
	if err == nil || err.Error() != "test error" {
		t.Errorf("Expected 'test error', got %v", err)
	}
}
